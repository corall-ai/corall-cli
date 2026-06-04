import { materializeRuntimeConfig, resolveHooksToken, validateRuntimeConfig } from "./config.js";
import { fetchJson, fetchOk, isAbortError, sleep } from "./http.js";
import type {
  HookPayload,
  OpenClawConfig,
  PluginConfig,
  PollingEvent,
  ReadyRuntimeConfig,
  RuntimeConfig,
  Logger,
} from "./types.js";

interface PollingApi {
  config: OpenClawConfig;
  logger: Logger;
}

interface PollingService {
  start(): Promise<void>;
  stop(): Promise<void>;
}

interface CreatePollingServiceOptions {
  api: PollingApi;
  config: PluginConfig;
}

type JsonObject = Record<string, unknown>;

function asObject(value: unknown): JsonObject | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonObject)
    : null;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function isHookPayload(value: unknown): value is HookPayload {
  const hook = asObject(value);
  return Boolean(
    hook &&
      typeof hook.message === "string" &&
      typeof hook.name === "string" &&
      typeof hook.sessionKey === "string" &&
      typeof hook.deliver === "boolean",
  );
}

function buildAuthHeaders(token: string): Record<string, string> {
  return {
    authorization: `Bearer ${token}`,
    accept: "application/json",
  };
}

function extractEvents(payload: unknown): unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  const objectPayload = asObject(payload);
  if (!objectPayload) {
    return [];
  }

  if (Array.isArray(objectPayload.events)) {
    return objectPayload.events;
  }

  if (objectPayload.event) {
    return [objectPayload.event];
  }

  if (objectPayload.hook) {
    return [objectPayload];
  }

  return [];
}

function normalizeEvent(value: unknown): PollingEvent | null {
  const raw = asObject(value);
  if (!raw) {
    return null;
  }

  const hook = raw.hook;
  const id = asString(raw.id) ?? asString(raw.streamId) ?? asString(raw.stream_id);
  if (!id || !isHookPayload(hook)) {
    return null;
  }

  const dedupeId =
    asString(raw.eventId) ??
    asString(raw.event_id) ??
    asString(raw.dedupeId) ??
    asString(raw.dedupe_id) ??
    hook.sessionKey ??
    id;

  return {
    id,
    dedupeId,
    hook,
  };
}

function isPollingEvent(event: PollingEvent | null): event is PollingEvent {
  return event !== null;
}

function pruneRecentEvents(recentEvents: Map<string, number>, ttlMs: number): void {
  const cutoff = Date.now() - ttlMs;
  for (const [eventId, timestamp] of recentEvents.entries()) {
    if (timestamp < cutoff) {
      recentEvents.delete(eventId);
    }
  }
}

function readyConfigOrNull(config: RuntimeConfig): ReadyRuntimeConfig | null {
  if (!config.baseUrl || !config.agentId || !config.agentToken || !config.hookUrl) {
    return null;
  }

  return {
    ...config,
    baseUrl: config.baseUrl,
    agentId: config.agentId,
    agentToken: config.agentToken,
    hookUrl: config.hookUrl,
  };
}

async function pollEvents(
  config: ReadyRuntimeConfig,
  signal: AbortSignal,
): Promise<PollingEvent[]> {
  const url = new URL(`/v1/agents/${encodeURIComponent(config.agentId)}/events`, config.baseUrl);
  url.searchParams.set("consumerId", config.consumerId);
  url.searchParams.set("wait", String(config.waitSeconds));

  const payload = await fetchJson(url, {
    method: "GET",
    headers: buildAuthHeaders(config.agentToken),
    timeoutMs: config.requestTimeoutMs,
    signal,
  });

  return extractEvents(payload).map(normalizeEvent).filter(isPollingEvent);
}

async function ackEvent(
  config: ReadyRuntimeConfig,
  eventId: string,
  signal: AbortSignal,
): Promise<void> {
  const url = new URL(
    `/v1/agents/${encodeURIComponent(config.agentId)}/events/${encodeURIComponent(eventId)}/ack`,
    config.baseUrl,
  );

  await fetchOk(url, {
    method: "POST",
    headers: buildAuthHeaders(config.agentToken),
    timeoutMs: config.ackTimeoutMs,
    signal,
  });
}

async function forwardHook(
  hookUrl: string,
  hooksToken: string,
  hook: HookPayload,
  timeoutMs: number,
  signal: AbortSignal,
): Promise<void> {
  await fetchOk(new URL(hookUrl), {
    method: "POST",
    headers: {
      ...buildAuthHeaders(hooksToken),
      "content-type": "application/json",
    },
    body: JSON.stringify(hook),
    timeoutMs,
    signal,
  });
}

interface HandleEventOptions {
  api: PollingApi;
  config: ReadyRuntimeConfig;
  hooksToken: string;
  event: PollingEvent;
  recentEvents: Map<string, number>;
  signal: AbortSignal;
}

async function handleEvent({
  api,
  config,
  hooksToken,
  event,
  recentEvents,
  signal,
}: HandleEventOptions): Promise<void> {
  const alreadyForwarded = recentEvents.has(event.dedupeId);

  if (!alreadyForwarded) {
    await forwardHook(config.hookUrl, hooksToken, event.hook, config.ackTimeoutMs, signal);
    recentEvents.set(event.dedupeId, Date.now());
    api.logger.debug(
      `[corall-polling] Forwarded event ${event.dedupeId} (${event.id}) to ${config.hookUrl}`,
    );
  }

  await ackEvent(config, event.id, signal);
}

interface RunLoopOptions {
  api: PollingApi;
  config: ReadyRuntimeConfig;
  hooksToken: string;
  isCurrentConfig(): boolean;
  signal: AbortSignal;
  recentEvents: Map<string, number>;
}

interface SupervisorOptions {
  api: PollingApi;
  config: PluginConfig;
  signal: AbortSignal;
  recentEvents: Map<string, number>;
}

async function runLoop({
  api,
  config,
  hooksToken,
  isCurrentConfig,
  signal,
  recentEvents,
}: RunLoopOptions): Promise<void> {
  let backoffMs = config.errorBackoffMs;

  while (!signal.aborted) {
    if (!isCurrentConfig()) {
      api.logger.info("[corall-polling] Runtime config changed; restarting poller");
      return;
    }

    try {
      pruneRecentEvents(recentEvents, config.recentEventTtlMs);

      const events = await pollEvents(config, signal);
      if (events.length === 0) {
        backoffMs = config.errorBackoffMs;
        await sleep(config.idleDelayMs, signal);
        continue;
      }

      for (const event of events) {
        if (signal.aborted) {
          return;
        }
        await handleEvent({ api, config, hooksToken, event, recentEvents, signal });
      }

      backoffMs = config.errorBackoffMs;
    } catch (error: unknown) {
      if (isAbortError(error)) {
        return;
      }

      api.logger.warn(
        `[corall-polling] Poll cycle failed: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
      await sleep(backoffMs, signal);
      backoffMs = Math.min(
        Math.max(backoffMs * 2, config.errorBackoffMs),
        config.maxErrorBackoffMs,
      );
    }
  }
}

function runtimeIssueMessage(runtimeErrors: string[], readyConfig: ReadyRuntimeConfig | null): string {
  if (runtimeErrors.length > 0) {
    return runtimeErrors.join("; ");
  }

  if (!readyConfig) {
    return "runtime config is incomplete";
  }

  return "unknown runtime issue";
}

function sameReadyConfig(left: ReadyRuntimeConfig, right: ReadyRuntimeConfig): boolean {
  return (
    left.baseUrl === right.baseUrl &&
    left.agentId === right.agentId &&
    left.agentToken === right.agentToken &&
    left.consumerId === right.consumerId &&
    left.hookUrl === right.hookUrl
  );
}

async function runSupervisor({
  api,
  config,
  signal,
  recentEvents,
}: SupervisorOptions): Promise<void> {
  let lastIssue: string | null = null;

  while (!signal.aborted) {
    const runtimeConfig = materializeRuntimeConfig(config, api.config);
    const runtimeErrors = validateRuntimeConfig(runtimeConfig, api.config);
    const hooksToken = resolveHooksToken(api.config);
    const readyConfig = readyConfigOrNull(runtimeConfig);

    if (runtimeErrors.length > 0 || !hooksToken || !readyConfig) {
      const issue = runtimeIssueMessage(runtimeErrors, readyConfig);
      if (issue !== lastIssue) {
        api.logger.warn(`[corall-polling] Waiting for config: ${issue}`);
        lastIssue = issue;
      }
      await sleep(runtimeConfig.idleDelayMs, signal);
      continue;
    }

    api.logger.info(
      `[corall-polling] Starting poller for agent ${readyConfig.agentId} using consumer ${readyConfig.consumerId}`,
    );

    await runLoop({
      api,
      config: readyConfig,
      hooksToken,
      isCurrentConfig: () => {
        const currentConfig = readyConfigOrNull(materializeRuntimeConfig(config, api.config));
        return currentConfig !== null && sameReadyConfig(currentConfig, readyConfig);
      },
      signal,
      recentEvents,
    });
  }
}

export function createPollingService({ api, config }: CreatePollingServiceOptions): PollingService {
  let loopPromise: Promise<void> | null = null;
  let stopController: AbortController | null = null;
  const recentEvents = new Map<string, number>();

  return {
    async start(): Promise<void> {
      if (loopPromise) {
        return;
      }

      stopController = new AbortController();

      loopPromise = runSupervisor({
        api,
        config,
        signal: stopController.signal,
        recentEvents,
      })
        .catch((error: unknown) => {
          if (!isAbortError(error)) {
            api.logger.error(
              `[corall-polling] Poller stopped unexpectedly: ${
                error instanceof Error ? error.message : String(error)
              }`,
            );
          }
        })
        .finally(() => {
          loopPromise = null;
          stopController = null;
          recentEvents.clear();
        });
    },

    async stop(): Promise<void> {
      if (!loopPromise || !stopController) {
        return;
      }

      stopController.abort();

      try {
        await loopPromise;
      } catch (error: unknown) {
        if (!isAbortError(error)) {
          throw error;
        }
      }
    },
  };
}

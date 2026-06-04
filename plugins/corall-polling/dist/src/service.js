import { materializeRuntimeConfig, resolveHooksToken, validateRuntimeConfig } from "./config.js";
import { fetchJson, fetchOk, isAbortError, sleep } from "./http.js";
function asObject(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value)
        ? value
        : null;
}
function asString(value) {
    return typeof value === "string" && value.trim() ? value.trim() : undefined;
}
function isHookPayload(value) {
    const hook = asObject(value);
    return Boolean(hook &&
        typeof hook.message === "string" &&
        typeof hook.name === "string" &&
        typeof hook.sessionKey === "string" &&
        typeof hook.deliver === "boolean");
}
function buildAuthHeaders(token) {
    return {
        authorization: `Bearer ${token}`,
        accept: "application/json",
    };
}
function extractEvents(payload) {
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
function normalizeEvent(value) {
    const raw = asObject(value);
    if (!raw) {
        return null;
    }
    const hook = raw.hook;
    const id = asString(raw.id) ?? asString(raw.streamId) ?? asString(raw.stream_id);
    if (!id || !isHookPayload(hook)) {
        return null;
    }
    const dedupeId = asString(raw.eventId) ??
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
function isPollingEvent(event) {
    return event !== null;
}
function pruneRecentEvents(recentEvents, ttlMs) {
    const cutoff = Date.now() - ttlMs;
    for (const [eventId, timestamp] of recentEvents.entries()) {
        if (timestamp < cutoff) {
            recentEvents.delete(eventId);
        }
    }
}
function readyConfigOrNull(config) {
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
async function pollEvents(config, signal) {
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
async function ackEvent(config, eventId, signal) {
    const url = new URL(`/v1/agents/${encodeURIComponent(config.agentId)}/events/${encodeURIComponent(eventId)}/ack`, config.baseUrl);
    await fetchOk(url, {
        method: "POST",
        headers: buildAuthHeaders(config.agentToken),
        timeoutMs: config.ackTimeoutMs,
        signal,
    });
}
async function forwardHook(hookUrl, hooksToken, hook, timeoutMs, signal) {
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
async function handleEvent({ api, config, hooksToken, event, recentEvents, signal, }) {
    const alreadyForwarded = recentEvents.has(event.dedupeId);
    if (!alreadyForwarded) {
        await forwardHook(config.hookUrl, hooksToken, event.hook, config.ackTimeoutMs, signal);
        recentEvents.set(event.dedupeId, Date.now());
        api.logger.debug(`[corall-polling] Forwarded event ${event.dedupeId} (${event.id}) to ${config.hookUrl}`);
    }
    await ackEvent(config, event.id, signal);
}
async function runLoop({ api, config, hooksToken, isCurrentConfig, signal, recentEvents, }) {
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
        }
        catch (error) {
            if (isAbortError(error)) {
                return;
            }
            api.logger.warn(`[corall-polling] Poll cycle failed: ${error instanceof Error ? error.message : String(error)}`);
            await sleep(backoffMs, signal);
            backoffMs = Math.min(Math.max(backoffMs * 2, config.errorBackoffMs), config.maxErrorBackoffMs);
        }
    }
}
function runtimeIssueMessage(runtimeErrors, readyConfig) {
    if (runtimeErrors.length > 0) {
        return runtimeErrors.join("; ");
    }
    if (!readyConfig) {
        return "runtime config is incomplete";
    }
    return "unknown runtime issue";
}
function sameReadyConfig(left, right) {
    return (left.baseUrl === right.baseUrl &&
        left.agentId === right.agentId &&
        left.agentToken === right.agentToken &&
        left.consumerId === right.consumerId &&
        left.hookUrl === right.hookUrl);
}
async function runSupervisor({ api, config, signal, recentEvents, }) {
    let lastIssue = null;
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
        api.logger.info(`[corall-polling] Starting poller for agent ${readyConfig.agentId} using consumer ${readyConfig.consumerId}`);
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
export function createPollingService({ api, config }) {
    let loopPromise = null;
    let stopController = null;
    const recentEvents = new Map();
    return {
        async start() {
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
                .catch((error) => {
                if (!isAbortError(error)) {
                    api.logger.error(`[corall-polling] Poller stopped unexpectedly: ${error instanceof Error ? error.message : String(error)}`);
                }
            })
                .finally(() => {
                loopPromise = null;
                stopController = null;
                recentEvents.clear();
            });
        },
        async stop() {
            if (!loopPromise || !stopController) {
                return;
            }
            stopController.abort();
            try {
                await loopPromise;
            }
            catch (error) {
                if (!isAbortError(error)) {
                    throw error;
                }
            }
        },
    };
}
//# sourceMappingURL=service.js.map
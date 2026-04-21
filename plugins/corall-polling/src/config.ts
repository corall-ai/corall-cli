import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import type { OpenClawConfig, PluginConfig, RuntimeConfig } from "./types.js";

const DEFAULT_GATEWAY_PORT = 18789;
const DEFAULT_HOOK_URL_PATH = "/hooks/agent";
const DEFAULT_WAIT_SECONDS = 30;
const DEFAULT_IDLE_DELAY_MS = 1000;
const DEFAULT_ACK_TIMEOUT_MS = 10_000;
const DEFAULT_ERROR_BACKOFF_MS = 2_000;
const DEFAULT_MAX_ERROR_BACKOFF_MS = 30_000;
const DEFAULT_RECENT_EVENT_TTL_MS = 10 * 60 * 1000;
const DEFAULT_CREDENTIAL_PROFILE = "provider";

type JsonObject = Record<string, unknown>;

function asObject(value: unknown): JsonObject {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonObject)
    : {};
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function asInteger(value: unknown, fallback: number): number {
  return Number.isInteger(value) && typeof value === "number" && value >= 0 ? value : fallback;
}

function stripTrailingSlashes(value: string): string {
  return value.replace(/\/+$/, "");
}

export function resolvePluginConfig(rawValue: unknown): PluginConfig {
  const raw = asObject(rawValue);
  const waitSeconds = Math.min(asInteger(raw.waitSeconds, DEFAULT_WAIT_SECONDS), 60);
  const rawAgentId = asString(raw.agentId);
  const rawBaseUrl = asString(raw.baseUrl);
  const rawConsumerId = asString(raw.consumerId);

  return {
    baseUrl: rawBaseUrl ? stripTrailingSlashes(rawBaseUrl) : undefined,
    agentId: rawAgentId,
    agentToken: asString(raw.agentToken),
    credentialProfile: asString(raw.credentialProfile) ?? DEFAULT_CREDENTIAL_PROFILE,
    consumerId: rawConsumerId,
    waitSeconds,
    hookUrl: asString(raw.hookUrl),
    requestTimeoutMs: Math.max(
      asInteger(raw.requestTimeoutMs, waitSeconds * 1000 + 15_000),
      waitSeconds * 1000 + 1_000,
    ),
    ackTimeoutMs: asInteger(raw.ackTimeoutMs, DEFAULT_ACK_TIMEOUT_MS),
    idleDelayMs: asInteger(raw.idleDelayMs, DEFAULT_IDLE_DELAY_MS),
    errorBackoffMs: asInteger(raw.errorBackoffMs, DEFAULT_ERROR_BACKOFF_MS),
    maxErrorBackoffMs: Math.max(
      asInteger(raw.maxErrorBackoffMs, DEFAULT_MAX_ERROR_BACKOFF_MS),
      asInteger(raw.errorBackoffMs, DEFAULT_ERROR_BACKOFF_MS),
    ),
    recentEventTtlMs: asInteger(raw.recentEventTtlMs, DEFAULT_RECENT_EVENT_TTL_MS),
  };
}

export function materializeRuntimeConfig(
  pluginConfig: PluginConfig,
  openclawConfig: OpenClawConfig,
): RuntimeConfig {
  const agentId = pluginConfig.agentId ?? readAgentIdFromCredentials(pluginConfig.credentialProfile);
  const agentToken = pluginConfig.agentToken ?? resolveHooksToken(openclawConfig);
  const consumerId =
    pluginConfig.consumerId ?? `corall-polling:${agentId ?? "unknown"}:${os.hostname()}`;

  return {
    ...pluginConfig,
    agentId,
    agentToken,
    consumerId,
    hookUrl: resolveHookUrl(openclawConfig, pluginConfig),
  };
}

export function resolveHookUrl(openclawConfig: OpenClawConfig, pluginConfig: PluginConfig): string {
  if (pluginConfig.hookUrl) {
    return pluginConfig.hookUrl;
  }

  const gateway = asObject(openclawConfig.gateway);
  const port = asInteger(gateway.port, DEFAULT_GATEWAY_PORT);
  return `http://127.0.0.1:${port}${DEFAULT_HOOK_URL_PATH}`;
}

export function resolveHooksToken(openclawConfig: OpenClawConfig): string | undefined {
  const hooks = asObject(openclawConfig.hooks);
  return asString(hooks.token);
}

export function validateRuntimeConfig(
  runtimeConfig: RuntimeConfig,
  openclawConfig: OpenClawConfig,
): string[] {
  const errors: string[] = [];

  if (!runtimeConfig.baseUrl) {
    errors.push("config.baseUrl is required");
  }
  if (!runtimeConfig.agentId) {
    errors.push("config.agentId is required or must exist in ~/.corall/credentials/<profile>.json");
  }
  if (!runtimeConfig.agentToken) {
    errors.push("config.agentToken is required or must match hooks.token");
  }
  if (!resolveHooksToken(openclawConfig)) {
    errors.push("hooks.token is missing from the active OpenClaw config");
  }

  return errors;
}

function readAgentIdFromCredentials(profile: string): string | undefined {
  const credentialsPath = path.join(os.homedir(), ".corall", "credentials", `${profile}.json`);

  try {
    const raw = fs.readFileSync(credentialsPath, "utf8");
    const parsed = JSON.parse(raw) as unknown;
    return asString(asObject(parsed).agentId);
  } catch {
    return undefined;
  }
}

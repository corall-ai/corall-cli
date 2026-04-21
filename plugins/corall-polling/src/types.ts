export interface Logger {
  debug(message: string): void;
  info(message: string): void;
  warn(message: string): void;
  error(message: string): void;
}

export interface OpenClawConfig {
  gateway?: {
    port?: unknown;
  };
  hooks?: {
    token?: unknown;
  };
}

export interface RegisteredService {
  id: string;
  start(): Promise<void>;
  stop(): Promise<void>;
}

export interface OpenClawPluginApi {
  pluginConfig: unknown;
  config: OpenClawConfig;
  logger: Logger;
  registerService(service: RegisteredService): void;
}

export interface PluginConfig {
  baseUrl: string | undefined;
  agentId: string | undefined;
  agentToken: string | undefined;
  credentialProfile: string;
  consumerId: string | undefined;
  waitSeconds: number;
  hookUrl: string | undefined;
  requestTimeoutMs: number;
  ackTimeoutMs: number;
  idleDelayMs: number;
  errorBackoffMs: number;
  maxErrorBackoffMs: number;
  recentEventTtlMs: number;
}

export interface RuntimeConfig extends PluginConfig {
  agentId: string | undefined;
  agentToken: string | undefined;
  consumerId: string;
  hookUrl: string;
}

export interface ReadyRuntimeConfig extends RuntimeConfig {
  baseUrl: string;
  agentId: string;
  agentToken: string;
  hookUrl: string;
}

export interface HookPayload {
  message: string;
  name: string;
  sessionKey: string;
  deliver: boolean;
}

export interface PollingEvent {
  id: string;
  dedupeId: string;
  hook: HookPayload;
}

import type { OpenClawConfig, PluginConfig, Logger } from "./types.js";
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
export declare function createPollingService({ api, config }: CreatePollingServiceOptions): PollingService;
export {};

import type { OpenClawConfig, PluginConfig, RuntimeConfig } from "./types.js";
export declare function resolvePluginConfig(rawValue: unknown): PluginConfig;
export declare function materializeRuntimeConfig(pluginConfig: PluginConfig, openclawConfig: OpenClawConfig): RuntimeConfig;
export declare function resolveHookUrl(openclawConfig: OpenClawConfig, pluginConfig: PluginConfig): string;
export declare function resolveHooksToken(openclawConfig: OpenClawConfig): string | undefined;
export declare function validateRuntimeConfig(runtimeConfig: RuntimeConfig, openclawConfig: OpenClawConfig): string[];

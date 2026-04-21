declare module "openclaw/plugin-sdk/plugin-entry" {
  import type { OpenClawPluginApi } from "./types.js";

  export interface PluginEntry {
    id: string;
    name: string;
    description: string;
    register(api: OpenClawPluginApi): void;
  }

  export function definePluginEntry(entry: PluginEntry): PluginEntry;
}

import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry";

import { resolvePluginConfig } from "./src/config.js";
import { createPollingService } from "./src/service.js";
import type { OpenClawPluginApi } from "./src/types.js";

export default definePluginEntry({
  id: "corall-polling",
  name: "Corall Polling",
  description:
    "Poll Corall resident events and forward hook payloads to the local OpenClaw hook endpoint.",
  register(api: OpenClawPluginApi): void {
    const config = resolvePluginConfig(api.pluginConfig);
    const service = createPollingService({ api, config });

    api.registerService({
      id: "corall-polling",
      start: service.start,
      stop: service.stop,
    });
  },
});

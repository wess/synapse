// The guidance that explains the tools, when nothing else is carrying it.
//
// A machine where `synapse connect pi` has run already has this: the managed
// block in `~/.pi/agent/APPEND_SYSTEM.md` points every session at SOUL.md, and
// SOUL.md is where the server's instructions come from in the first place.
// Installing the package on its own skips that step, and tools that arrive
// without the guidance that explains them get used badly or not at all — so the
// extension carries it, and stands down as soon as the pointer is there.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import type { Client } from "./client.ts";

/** What a system prompt that already carries Synapse guidance mentions. */
const POINTER = "SOUL.md";

export function guidance(pi: ExtensionAPI, client: Client): void {
  pi.on("before_agent_start", (event) => {
    if (client.instructions.length === 0 || event.systemPrompt.includes(POINTER)) {
      return;
    }
    return { systemPrompt: `${event.systemPrompt}\n\n${client.instructions}` };
  });
}

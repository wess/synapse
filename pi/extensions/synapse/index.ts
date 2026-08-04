// Synapse, as a pi extension.
//
// One `synapse mcp` process per session, every tool it advertises registered as
// a pi tool, this project's memory in context before the first turn, and the
// guidance that explains all of it. Nothing is hard-coded about which tools
// exist: Synapse decides that, and it changes when the user turns the mesh on.
//
// If Synapse is not installed, or the server will not start, the session says
// so once and carries on with no Synapse tools at all. Reporting a connection
// that is not there is the one failure mode worth designing against — an agent
// that believes it has durable memory and does not is worse off than one that
// knows it has none.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { binary } from "./binary.ts";
import { connect } from "./client.ts";
import { commands } from "./commands.ts";
import { guidance } from "./guidance.ts";
import { notice } from "./session.ts";
import { tools } from "./tools.ts";

export default async function (pi: ExtensionAPI): Promise<void> {
  // Whatever started this session decides which project it belongs to; a
  // session started by `synapse launch pi` is told, and one started by hand is
  // wherever the person is standing.
  const root = process.env.SYNAPSE_PROJECT_DIR ?? process.cwd();
  const command = binary();
  if (!command) {
    unavailable(pi, "the synapse command is not on PATH");
    return;
  }

  let client;
  try {
    // Awaited in the factory on purpose. pi finishes async factories before the
    // session starts, and the tool list is the answer to a question asked over
    // there — so this is the one moment where waiting buys a complete tool set.
    client = await connect(command, root);
  } catch (error) {
    unavailable(pi, (error as Error).message);
    return;
  }

  tools(pi, client);
  guidance(pi, client);
  notice(pi, command, root);
  commands(pi, client, command, root);

  // Shutdown covers quit, reload, and session replacement. A fresh runtime
  // starts its own server; this one takes its child with it either way.
  pi.on("session_shutdown", () => client.close());
}

function unavailable(pi: ExtensionAPI, why: string): void {
  pi.on("session_start", (_event, ctx) => {
    ctx.ui.notify(`Synapse unavailable · ${why}`, "warning");
    ctx.ui.setStatus("synapse", "◆ Synapse unavailable");
  });
}

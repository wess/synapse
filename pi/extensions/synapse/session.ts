// What the session shows, and what it starts already knowing.
//
// Two halves of the same answer, both produced by `synapse session`: the user
// gets one line saying the connection is real, and the model gets this
// project's memory handed to it before the first turn. Asking a model to recall
// on its own is guidance it may or may not follow, and a session that skips it
// works from nothing while reporting a connection.
//
// The status line is the same one Claude Code shows, printed by the same
// command, refreshed when the agent settles rather than on a timer.

import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { run } from "./command.ts";

const STATUS = "synapse";

export function notice(pi: ExtensionAPI, command: string, root: string): void {
  pi.on("session_start", async (_event, ctx) => {
    const opening = await start(command, root);
    ctx.ui.notify(opening.headline, opening.reachable ? "info" : "warning");
    if (opening.context.length > 0) {
      // display: false — the user has already read the headline, and the memory
      // itself is for the model. It reaches the model as context all the same.
      pi.sendMessage({ customType: STATUS, content: opening.context, display: false });
    }
    await refresh(ctx, command, root);
  });

  pi.on("agent_settled", async (_event, ctx) => {
    await refresh(ctx, command, root);
  });
}

interface Opening {
  headline: string;
  context: string;
  reachable: boolean;
}

/**
 * The session hook's own payload: `systemMessage` for the person, and
 * `additionalContext` for the model. Anything that goes wrong is reported as
 * what it is — a connection that is not there is never described as one that
 * is.
 */
async function start(command: string, root: string): Promise<Opening> {
  const ran = await run(command, ["session"], { cwd: root, input: cwd(root) });
  if (!ran.ok) {
    return { headline: `Synapse unavailable · ${trouble(ran.err)}`, context: "", reachable: false };
  }
  try {
    const payload = JSON.parse(ran.out) as {
      systemMessage?: string;
      hookSpecificOutput?: { additionalContext?: string };
    };
    return {
      headline: payload.systemMessage ?? "Synapse connected",
      context: payload.hookSpecificOutput?.additionalContext ?? "",
      reachable: true,
    };
  } catch {
    return { headline: "Synapse unavailable · unreadable session report", context: "", reachable: false };
  }
}

async function refresh(ctx: ExtensionContext, command: string, root: string): Promise<void> {
  const ran = await run(command, ["statusline"], { cwd: root, input: cwd(root) });
  const line = ran.out.trim();
  ctx.ui.setStatus(STATUS, ran.ok && line.length > 0 ? line : undefined);
}

function cwd(root: string): string {
  return JSON.stringify({ cwd: root });
}

function trouble(message: string): string {
  const first = message.trim().split("\n")[0] ?? "";
  return first.length > 0 ? first : "the synapse command could not be run";
}

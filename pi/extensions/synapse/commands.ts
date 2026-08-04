// The slash commands, for the times a person wants the answer themselves.
//
// Everything here the model can already do with a tool. These exist because
// "what does Synapse know about this project" and "who else is on the mesh" are
// questions somebody asks between turns, and spending a turn asking the model to
// ask the server is a poor way to find out.

import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";
import type { Client, Outcome } from "./client.ts";
import { run } from "./command.ts";
import { memories, roster, stored } from "./render.ts";

interface Report {
  memories: number;
  project: string | null;
  mesh: boolean;
  agents: number;
  vault: string;
  problem: string | null;
}

export function commands(pi: ExtensionAPI, client: Client, command: string, root: string): void {
  pi.registerCommand("synapse", {
    description: "Show what Synapse holds for this project",
    handler: async () => {
      say(pi, await state(client, command, root));
    },
  });

  pi.registerCommand("recall", {
    description: "Search durable memory for this project",
    handler: async (text, ctx) => {
      const query = text.trim();
      if (query.length === 0) {
        ctx.ui.notify("usage: /recall <what you are looking for>", "warning");
        return;
      }
      const answer = await ask(client, ctx, "recall", {
        query,
        project: root,
        limit: 8,
        budget: "lean",
      });
      say(pi, answer.failed ? answer.text : memories(answer.structured, answer.text));
    },
  });

  pi.registerCommand("remember", {
    description: "Store one durable fact for this project",
    handler: async (text, ctx) => {
      const content = text.trim();
      if (content.length === 0) {
        ctx.ui.notify("usage: /remember <the fact worth keeping>", "warning");
        return;
      }
      const answer = await ask(client, ctx, "remember", { content, project: root, source: "pi" });
      ctx.ui.notify(
        answer.failed ? answer.text : stored(answer.structured, answer.text),
        answer.failed ? "error" : "info",
      );
    },
  });

  pi.registerCommand("mesh", {
    description: "Show who is on the agent mesh",
    handler: async (_text, ctx) => {
      if (!client.tools.some((tool) => tool.name === "agents")) {
        ctx.ui.notify("The agent mesh is off. Turn it on with `synapse settings mesh on`.", "info");
        return;
      }
      const answer = await ask(client, ctx, "agents", {});
      say(pi, answer.failed ? answer.text : roster(answer.structured, answer.text));
    },
  });
}

/** Call one tool on the user's behalf. A failure comes back as text to show,
 * never as a throw: a slash command that raises takes the session down with it. */
async function ask(
  client: Client,
  ctx: ExtensionCommandContext,
  name: string,
  input: Record<string, unknown>,
): Promise<Outcome> {
  try {
    return await client.call(name, input, ctx.signal);
  } catch (error) {
    return { text: `Synapse could not answer: ${(error as Error).message}`, structured: undefined, failed: true };
  }
}

async function state(client: Client, command: string, root: string): Promise<string> {
  const ran = await run(command, ["session", "--json"], {
    cwd: root,
    input: JSON.stringify({ cwd: root }),
  });
  if (!ran.ok) {
    return `Synapse unavailable · ${ran.err.trim().split("\n")[0] ?? "the command failed"}`;
  }
  let report: Report;
  try {
    report = JSON.parse(ran.out) as Report;
  } catch {
    return "Synapse unavailable · unreadable report";
  }
  if (report.problem) {
    return `Synapse unavailable · ${report.problem}`;
  }
  return [
    `Synapse ${client.version}`,
    `Memory · ${report.memories} ${report.memories === 1 ? "memory" : "memories"}${
      report.project ? ` · project ${report.project}` : " · no project here"
    }`,
    `Vault · ${report.vault}`,
    `Mesh · ${report.mesh ? `on · ${report.agents} reachable` : "off"}`,
    `Tools · ${client.tools.map((tool) => tool.name).join(", ")}`,
  ].join("\n");
}

function say(pi: ExtensionAPI, content: string): void {
  pi.sendMessage({ customType: "synapse", content, display: true });
}

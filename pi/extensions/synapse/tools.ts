// Every tool the server advertises, offered to the model as a pi tool.
//
// Nothing here is a list of tool names. Synapse decides what an agent may reach
// — memory always, the sixteen mesh tools only while the mesh setting is on —
// and this file forwards that decision unchanged, names and descriptions
// included. Those descriptions are the model-facing contract on the other side,
// and the mesh harness a launched agent opens with names its tools bare
// (`register`, `wait`), so renaming or prefixing them here would break agents
// that Synapse itself started.

import type { ExtensionAPI, ToolDefinition } from "@earendil-works/pi-coding-agent";
import type { Client } from "./client.ts";

type Schema = ToolDefinition["parameters"];

export function tools(pi: ExtensionAPI, client: Client): void {
  for (const tool of client.tools) {
    pi.registerTool({
      name: tool.name,
      label: `synapse ${tool.name}`,
      description: tool.description,
      promptSnippet: `${tool.name}: ${summary(tool.description)}`,
      // The server's JSON Schema, verbatim. pi validates plain JSON Schema as
      // readily as it does its own, so there is nothing to translate and
      // nothing to drift.
      parameters: tool.schema as unknown as Schema,
      async execute(_id, parameters, signal) {
        const outcome = await client.call(tool.name, (parameters ?? {}) as Record<string, unknown>, signal);
        // The server's own verdict, not ours. A refusal is worth reading — it
        // is where "you are not on the mesh yet" comes from — so it becomes the
        // error text rather than a result the model has to interpret.
        if (outcome.failed) {
          throw new Error(outcome.text || `${tool.name} failed`);
        }
        return {
          content: [{ type: "text", text: outcome.text }],
          details: outcome.structured,
        };
      },
    });
  }
}

/** The first sentence of a description, for the one-line tool list. */
function summary(description: string): string {
  const first = description.trim().split(/(?<=\.)\s/)[0] ?? description.trim();
  return first.length > 0 ? first : "a Synapse tool";
}

// Turning a tool's answer into something a person reads.
//
// The tools answer in JSON because a model reads them. A slash command is the
// one path where a person is the reader, so these four take the same structured
// answer and lay it out — and fall back to the raw text whenever the shape is
// not what this expects, because a readable-but-unstyled answer beats an empty
// one.

export interface Memory {
  id: number;
  body: string;
  source: string;
  scope: string;
  project: string;
  created: number;
}

export interface Agent {
  name: string;
  role: string;
  status: string;
  note: string;
  human: boolean;
  online: boolean;
  tool: string;
}

export function memories(structured: unknown, fallback: string): string {
  const found = (structured as { memories?: Memory[] } | undefined)?.memories;
  if (!Array.isArray(found)) {
    return fallback;
  }
  if (found.length === 0) {
    return "Nothing stored for this project yet.";
  }
  const header = `${found.length} ${found.length === 1 ? "memory" : "memories"}, newest first`;
  return [header, "", ...found.map(entry)].join("\n");
}

export function roster(structured: unknown, fallback: string): string {
  const found = (structured as { agents?: Agent[] } | undefined)?.agents;
  if (!Array.isArray(found)) {
    return fallback;
  }
  if (found.length === 0) {
    return "The mesh is on and nobody has registered yet.";
  }
  return found.map(member).join("\n");
}

export function stored(structured: unknown, fallback: string): string {
  const id = (structured as { id?: number } | undefined)?.id;
  return typeof id === "number" ? `Remembered · #${id}` : fallback;
}

/** One memory. Bodies run to several lines often enough that indenting the rest
 * matters: an unindented second line reads as a second memory. */
function entry(memory: Memory): string {
  const [first, ...rest] = memory.body.trim().split("\n");
  const lines = [`- ${first}`, ...rest.map((line) => `  ${line}`)];
  const source = memory.source.trim();
  const marks = [source, memory.scope === "global" ? "global" : undefined, when(memory.created)]
    .filter((mark): mark is string => Boolean(mark))
    .join(", ");
  if (marks.length > 0) {
    lines.push(`  (${marks})`);
  }
  return lines.join("\n");
}

function member(agent: Agent): string {
  const marks = [agent.human ? "person" : agent.tool, agent.online ? "reachable" : "away"]
    .filter((mark) => mark.length > 0)
    .join(", ");
  const state = [agent.status, agent.note].filter((part) => part.length > 0).join(" · ");
  return `- ${agent.name} (${agent.role}) [${marks}]${state.length > 0 ? ` — ${state}` : ""}`;
}

function when(created: number): string | undefined {
  if (!Number.isFinite(created) || created <= 0) {
    return undefined;
  }
  return new Date(created * 1000).toISOString().slice(0, 10);
}

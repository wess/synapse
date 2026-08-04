// A Model Context Protocol client over `synapse mcp`.
//
// This is the whole reason the package exists. pi has no MCP client of its own,
// and every Synapse capability an agent can reach — memory, vault metadata, the
// mesh — lives behind that one stdio server. Talking to it directly, rather than
// re-implementing the commands, means the tool set this extension offers is
// whatever the installed Synapse offers: turn the mesh on and sixteen more tools
// appear here on the next start, with no version of this file to update.
//
// The transport is newline-delimited JSON-RPC on the child's stdio, and replies
// are matched by id. That matters more than it sounds: a mesh agent parks on
// `wait` for minutes at a time, and nothing else may stop while it is out.

import { spawn } from "node:child_process";

/** One tool as the server advertises it. The schema is passed to pi verbatim. */
export interface Tool {
  name: string;
  description: string;
  schema: Record<string, unknown>;
}

/** What one call came back with. `failed` is the server's own verdict. */
export interface Outcome {
  text: string;
  structured: unknown;
  failed: boolean;
}

export interface Client {
  readonly tools: Tool[];
  /** The server's model-facing guidance, sourced from SOUL.md. */
  readonly instructions: string;
  readonly version: string;
  call(name: string, input: Record<string, unknown>, signal?: AbortSignal): Promise<Outcome>;
  close(): void;
}

const PROTOCOL = "2025-06-18";
/** How long the handshake may take before the server is treated as broken. A
 * call has no deadline at all — parking is a feature — but a start that hangs
 * would hang pi's start with it. */
const HANDSHAKE = 15_000;
/** How much of the server's stderr to keep for the message on a failure. */
const STDERRKEEP = 2_000;

type Waiting = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
};

/**
 * Start the server and complete the handshake. Rejects when the binary cannot
 * run or does not answer, which the caller reports rather than hides: a session
 * that says it has memory and does not is worse than one that says it has none.
 */
export async function connect(command: string, root: string): Promise<Client> {
  const child = spawn(command, ["mcp"], {
    cwd: root,
    env: { ...process.env, SYNAPSE_PROJECT_DIR: root },
    stdio: ["pipe", "pipe", "pipe"],
  });

  const waiting = new Map<number, Waiting>();
  let identifier = 0;
  let buffer = "";
  let complaints = "";
  let stopped: Error | undefined;

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => {
    buffer += chunk;
    let at = buffer.indexOf("\n");
    while (at >= 0) {
      const line = buffer.slice(0, at).trim();
      buffer = buffer.slice(at + 1);
      if (line.length > 0) {
        answer(line);
      }
      at = buffer.indexOf("\n");
    }
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    complaints = (complaints + chunk).slice(-STDERRKEEP);
  });
  child.on("error", (error: Error) => stop(error));
  child.on("exit", (code) => stop(new Error(ended(code, complaints))));
  // Writing to a server that has already gone is how this fails in practice,
  // and an unhandled stream error would take the whole session down with it.
  // The exit that caused it is what gets reported.
  child.stdin.on("error", () => {});

  function answer(line: string): void {
    let message: { id?: number; result?: unknown; error?: { message?: string } };
    try {
      message = JSON.parse(line);
    } catch {
      // Anything the server writes that is not a message is not ours to
      // interpret, and dropping it keeps one stray line from ending a session.
      return;
    }
    if (typeof message.id !== "number") {
      return;
    }
    const pending = waiting.get(message.id);
    if (!pending) {
      return;
    }
    waiting.delete(message.id);
    if (message.error) {
      pending.reject(new Error(message.error.message ?? "the Synapse server refused the call"));
      return;
    }
    pending.resolve(message.result);
  }

  function stop(error: Error): void {
    stopped = error;
    for (const pending of waiting.values()) {
      pending.reject(error);
    }
    waiting.clear();
  }

  function write(message: Record<string, unknown>): void {
    child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  function request(
    method: string,
    parameters: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<unknown> {
    if (stopped) {
      return Promise.reject(stopped);
    }
    const id = ++identifier;
    return new Promise<unknown>((resolve, reject) => {
      waiting.set(id, { resolve, reject });
      if (signal) {
        signal.addEventListener(
          "abort",
          () => {
            if (!waiting.delete(id)) {
              return;
            }
            // Tell the server to drop the work as well. A parked `wait` that
            // nobody is listening for would otherwise hold a database read for
            // its whole timeout.
            write({
              jsonrpc: "2.0",
              method: "notifications/cancelled",
              params: { requestId: id, reason: "the session cancelled this call" },
            });
            reject(new Error("cancelled"));
          },
          { once: true },
        );
      }
      write({ jsonrpc: "2.0", id, method, params: parameters });
    });
  }

  const started = await deadline(
    request("initialize", {
      protocolVersion: PROTOCOL,
      capabilities: {},
      clientInfo: { name: "pi", version: PROTOCOL },
    }),
    HANDSHAKE,
    () => child.kill(),
  );
  write({ jsonrpc: "2.0", method: "notifications/initialized" });
  const listed = await deadline(request("tools/list", {}), HANDSHAKE, () => child.kill());

  const information = started as {
    instructions?: string;
    serverInfo?: { version?: string };
  };
  const advertised = (listed as { tools?: unknown[] }).tools ?? [];

  return {
    tools: advertised.map(described).filter((tool): tool is Tool => tool !== undefined),
    instructions: information.instructions ?? "",
    version: information.serverInfo?.version ?? "",
    async call(name, input, signal) {
      const result = (await request("tools/call", { name, arguments: input }, signal)) as {
        content?: { type?: string; text?: string }[];
        structuredContent?: unknown;
        isError?: boolean;
      };
      const text = (result.content ?? [])
        .filter((part) => part.type === "text" && typeof part.text === "string")
        .map((part) => part.text)
        .join("\n");
      return {
        text,
        structured: result.structuredContent,
        failed: result.isError === true,
      };
    },
    close() {
      stop(new Error("the Synapse server was closed with this session"));
      child.kill();
    },
  };
}

function described(value: unknown): Tool | undefined {
  const tool = value as { name?: string; description?: string; inputSchema?: unknown };
  if (typeof tool.name !== "string" || typeof tool.inputSchema !== "object" || !tool.inputSchema) {
    return undefined;
  }
  return {
    name: tool.name,
    description: tool.description ?? "",
    schema: tool.inputSchema as Record<string, unknown>,
  };
}

function ended(code: number | null, complaints: string): string {
  const reason = complaints.trim().split("\n").at(-1);
  const detail = reason ? `: ${reason}` : "";
  return `the Synapse server stopped (exit ${code ?? "signal"})${detail}`;
}

async function deadline<T>(work: Promise<T>, milliseconds: number, cancel: () => void): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      cancel();
      reject(new Error("the Synapse server did not answer"));
    }, milliseconds);
    timer.unref?.();
  });
  try {
    return await Promise.race([work, expiry]);
  } finally {
    clearTimeout(timer);
  }
}

// Running the synapse command and reading what it printed.
//
// A handful of things a session needs are reports rather than tools — the
// startup notice, the status line, the roster behind `/mesh` — and they already
// exist as commands with their own wording. Shelling out to them keeps one
// source of truth for that wording instead of a second copy here that drifts.

import { spawn } from "node:child_process";

export interface Ran {
  ok: boolean;
  out: string;
  err: string;
}

export function run(
  command: string,
  parameters: string[],
  options: { cwd: string; input?: string },
): Promise<Ran> {
  return new Promise((resolve) => {
    const child = spawn(command, parameters, {
      cwd: options.cwd,
      env: { ...process.env, SYNAPSE_PROJECT_DIR: options.cwd },
      stdio: ["pipe", "pipe", "pipe"],
    });
    let out = "";
    let err = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      out += chunk;
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      err += chunk;
    });
    child.on("error", (error: Error) => resolve({ ok: false, out, err: error.message }));
    child.on("close", (code) => resolve({ ok: code === 0, out, err }));
    child.stdin.end(options.input ?? "");
  });
}

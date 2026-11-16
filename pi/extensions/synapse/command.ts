// reports reuse CLI wording instead of keeping a second copy here.

import { spawn } from "node:child_process";

export type Ran = {
  ok: boolean;
  out: string;
  err: string;
};

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

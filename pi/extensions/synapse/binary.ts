// Where the `synapse` command is.
//
// A session that Synapse itself started is told, because the binary that
// started it is the one whose memory this session belongs to. Everywhere else
// this is a PATH lookup plus the handful of places a single-user install lands,
// which is what makes the package work under a pi that was never started from a
// login shell. It mirrors the lookup the Rust side does when it detects a tool,
// so both halves agree on which binary is "the" one.

import { accessSync, constants } from "node:fs";
import { homedir } from "node:os";
import { delimiter, isAbsolute, join } from "node:path";

const COMMAND = "synapse";
const PERSONAL = [".local/bin", ".cargo/bin", ".asdf/shims"];
const SYSTEM = ["/opt/homebrew/bin", "/usr/local/bin"];

/** The synapse executable, or nothing when this machine has none. */
export function binary(): string | undefined {
  const named = process.env.SYNAPSE_COMMAND;
  if (named && isAbsolute(named) && runnable(named)) {
    return named;
  }
  const home = homedir();
  const directories = [
    ...(process.env.PATH ?? "").split(delimiter).filter((entry) => entry.length > 0),
    ...PERSONAL.map((entry) => join(home, entry)),
    ...SYSTEM,
  ];
  return directories.map((directory) => join(directory, COMMAND)).find(runnable);
}

function runnable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

// honor the launcher first, then mirror the Rust detector's PATH fallbacks.

import { accessSync, constants } from "node:fs";
import { homedir } from "node:os";
import { delimiter, isAbsolute, join } from "node:path";

const COMMAND = process.platform === "win32" ? "synapse.exe" : "synapse";
const PERSONAL = [".local/bin", ".cargo/bin", ".asdf/shims"];
const SYSTEM = ["/opt/homebrew/bin", "/usr/local/bin"];

export function binary(): string | undefined {
  const named = process.env.SYNAPSE_COMMAND;
  if (named && isAbsolute(named) && runnable(named)) {
    return named;
  }
  const home = homedir();
  const directories = [
    ...(process.env.PATH ?? "").split(delimiter).filter((entry) => entry.length > 0),
    ...PERSONAL.map((entry) => join(home, entry)),
    ...(process.platform === "win32"
      ? ["Programs/Synapse/bin", "synapse/bin"].map((entry) =>
          join(process.env.LOCALAPPDATA ?? join(home, "AppData/Local"), entry),
        )
      : []),
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

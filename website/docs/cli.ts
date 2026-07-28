import { command, note } from "../markup";
import type { Page } from "../types";

export const cli: Page = {
  path: "docs/cli/index.html",
  title: "CLI reference",
  description: "The complete Synaps command surface, including inputs, output modes, confirmation guards, and process behavior.",
  kind: "docs",
  toc: [
    { label: "Invocation", id: "invocation" },
    { label: "Application and server", id: "application" },
    { label: "Shell environments", id: "shell" },
    { label: "Vault and scope", id: "vault" },
    { label: "Memory", id: "memory" },
    { label: "Data and settings", id: "data" },
    { label: "Installation and paths", id: "install" },
  ],
  body: `
    <h2 id="invocation">Invocation</h2>
    <p>Run <code>synaps help</code> for the built-in summary. Commands print human-readable output unless a documented <code>--json</code> option is present. Errors go to stderr and return a non-zero exit status.</p>
    ${note("Secret and memory input", "Secret values are never accepted as arguments. Secret set uses a hidden prompt or stdin. Memory add and edit always read their body from stdin.")}

    <h2 id="application">Application and server</h2>
    ${command("app", "synaps app", "Open the native desktop application. Running <code>synaps</code with no command has the same result.")}
    ${command("mcp", "synaps mcp", "Run the MCP stdio server until the client closes the connection. The process holds a shared database lifecycle lock.")}
    ${command("run", "synaps run -- <command> [arguments]", "Resolve global, project, and folder vault mappings for the current directory, refuse on any scope warning, read selected Keychain values, and launch the child. Synaps returns the child exit code, or 1 when the operating system reports no code.")}
    ${command("status", "synaps status [folder] [--json]", "Show the resolved folder, available environment names, discovered scope states, warnings, ambient readiness, and detected shell hook. Values remain in Keychain. The folder defaults to the current directory.")}

    <h2 id="shell">Shell environments</h2>
    ${command("hook", "synaps hook <zsh|bash|fish>", "Print the integration script for a shell. Evaluate or source it from that shell’s startup file. The hook reevaluates approved directory scopes after directory changes and before prompts.")}
    ${command("allow", "synaps allow [folder]", "Inspect and approve the exact contents of the closest <code>.synaps.yaml</code> at or above the folder. The folder defaults to the current directory.")}
    ${command("deny", "synaps deny [folder]", "Revoke approval for the closest discovered scope. An installed hook unloads its managed values at the next prompt or directory change.")}
    ${command("export", "synaps export <zsh|bash|fish>", "Internal hook protocol. Emits a shell-quoted environment diff that the installed hook evaluates. Users normally invoke <code>hook</code>, <code>allow</code>, and <code>deny</code> instead.")}
    ${note("Two boundaries", "synaps run gives one child process the resolved environment. The shell hook exposes the environment to every process launched from an activated shell until the scope unloads.")}

    <h2 id="vault">Vault and scope</h2>
    ${command("vault list", "synaps vault list", "List vault names alphabetically.")}
    ${command("vault create", "synaps vault create <name>", "Create a unique vault. Names may contain ASCII letters, numbers, and hyphens.")}
    ${command("vault delete", "synaps vault delete <name>", "Delete an empty vault. Forget every contained secret first.")}
    ${command("secret list", "synaps secret list <vault>", "List each <code>vault.name</code> reference, environment name, and whether it is global or scoped. Never prints a value.")}
    ${command("secret set", "synaps secret set <vault> <name> <env> [--global]", "Read a value from a hidden terminal prompt or stdin and save it in Keychain. Creates metadata when the label is new; updates the existing Keychain item when the environment name matches. <code>--global</code> also enables the mapping.")}
    ${command("secret forget", "synaps secret forget <vault.name>", "Delete the Keychain value, its metadata, and any global mapping.")}
    ${command("secret global", "synaps secret global <vault.name> <on|off>", "Enable or disable a secret’s global environment mapping. Enabling replaces the current global source for that environment name.")}
    ${command("scope init", "synaps scope init [folder] [--folder]", "Create a new <code>.synaps.yaml</code>. The folder defaults to the current directory. Use <code>--folder</code> for a folder-kind template; the command refuses when the file already exists.")}
    ${command("scope trust", "synaps scope trust [folder]", "Approve an exact scope path directly by parsing it, calculating its content digest, and storing that digest with its canonical path. <code>synaps allow</code> is the directory-oriented shortcut.")}
    ${command("scope status", "synaps scope status [folder] [--json]", "Alias the scope-oriented status flow to the same resolved output as <code>synaps status</code>.")}

    <h2 id="memory">Memory</h2>
    ${command("memory list", "synaps memory list [query] [--json]", "Search the body with the joined query words or list recent memory when empty. Returns up to 100 entries. Text mode prints ID, source, and a compact preview.")}
    ${command("memory show", "synaps memory show <id> [--json]", "Print one exact memory with source and timestamp, or return structured JSON.")}
    ${command("memory add", "synaps memory add [source]", "Read a non-empty body from stdin and store it with an optional source label.")}
    ${command("memory edit", "synaps memory edit <id> [source]", "Read the replacement body from stdin and replace one existing memory. The optional source replaces the source label.")}
    ${command("memory delete", "synaps memory delete <id> --confirm", "Delete one memory. The exact <code>--confirm</code> guard is required.")}
    ${command("memory wipe", "synaps memory wipe --confirm", "Delete every memory entry while leaving settings, vaults, Keychain values, and scope approvals intact. The exact guard is required.")}

    <h2 id="data">Data and settings</h2>
    ${command("data check", "synaps data check [--json]", "Open the database, run page and foreign-key integrity checks, apply supported migrations, and report the path, schema version, and <code>ok</code> integrity state.")}
    ${command("data export", "synaps data export <file>", "Create and validate a consistent SQLite snapshot at a destination that does not already exist. Secret values remain in Keychain and are not included.")}
    ${command("data restore", "synaps data restore <file>", "Validate a current-version snapshot and restore it while the app and MCP servers are closed. Preserves the previous database as a recovery backup and refuses without the exclusive lock.")}
    ${command("settings show", "synaps settings show", "Print the active recall optimization, result limit, character budget, supported shell modes, and zsh hook example.")}
    ${command("settings optimize", "synaps settings optimize <full|balanced|lean>", "Change the shared MCP recall response budget. Stored memory is not modified.")}

    <h2 id="install">Installation and paths</h2>
    ${command("install", "synaps install", "Install the current executable for this user. A packaged app creates a launcher into the signed bundle; a development binary is copied atomically. Unrelated destination files are never overwritten.")}
    ${command("path", "synaps path", "Print the resolved home, data directory, database, and CLI destination.")}
    ${command("version", "synaps version", "Print the application version. <code>--version</code> and <code>-V</code> are aliases.")}
    ${command("help", "synaps help", "Print the command summary. <code>--help</code> and <code>-h</code> are aliases at the top level.")}
  `,
};

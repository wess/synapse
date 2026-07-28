import { command, note } from "../markup";
import type { Page } from "../types";

export const cli: Page = {
  path: "docs/cli/index.html",
  title: "CLI reference",
  description: "The complete Synapse command surface, including inputs, output modes, confirmation guards, and process behavior.",
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
    <p>Run <code>synapse help</code> for the built-in summary. Commands print human-readable output unless a documented <code>--json</code> option is present. Errors go to stderr and return a non-zero exit status.</p>
    ${note("Secret and memory input", "Secret values are never accepted as arguments. Secret set uses a hidden prompt or stdin. Memory add and edit always read their body from stdin.")}

    <h2 id="application">Application and server</h2>
    ${command("app", "synapse app", "Open the native desktop application. Running <code>synapse</code with no command has the same result.")}
    ${command("mcp", "synapse mcp", "Run the MCP stdio server until the client closes the connection. The process holds a shared database lifecycle lock.")}
    ${command("run", "synapse run -- <command> [arguments]", "Resolve global, project, and folder vault mappings for the current directory, refuse on any scope warning, read selected Keychain values, and launch the child. Synapse returns the child exit code, or 1 when the operating system reports no code.")}
    ${command("status", "synapse status [folder] [--json]", "Show the resolved folder, available environment names, discovered scope states, warnings, ambient readiness, and detected shell hook. Values remain in Keychain. The folder defaults to the current directory.")}

    <h2 id="shell">Shell environments</h2>
    ${command("hook", "synapse hook <zsh|bash|fish>", "Print the integration script for a shell. Evaluate or source it from that shell’s startup file. The hook reevaluates approved directory scopes after directory changes and before prompts.")}
    ${command("allow", "synapse allow [folder]", "Inspect and approve the exact contents of the closest <code>.synapse.yaml</code> at or above the folder. The folder defaults to the current directory.")}
    ${command("deny", "synapse deny [folder]", "Revoke approval for the closest discovered scope. An installed hook unloads its managed values at the next prompt or directory change.")}
    ${command("export", "synapse export <zsh|bash|fish>", "Internal hook protocol. Emits a shell-quoted environment diff that the installed hook evaluates. Users normally invoke <code>hook</code>, <code>allow</code>, and <code>deny</code> instead.")}
    ${note("Two boundaries", "synapse run gives one child process the resolved environment. The shell hook exposes the environment to every process launched from an activated shell until the scope unloads.")}

    <h2 id="vault">Vault and scope</h2>
    ${command("vault list", "synapse vault list", "List vault names alphabetically.")}
    ${command("vault create", "synapse vault create <name>", "Create a unique vault. Names may contain ASCII letters, numbers, and hyphens.")}
    ${command("vault delete", "synapse vault delete <name>", "Delete an empty vault. Forget every contained secret first.")}
    ${command("secret list", "synapse secret list <vault>", "List each <code>vault.name</code> reference, environment name, and whether it is global or scoped. Never prints a value.")}
    ${command("secret set", "synapse secret set <vault> <name> <env> [--global]", "Read a value from a hidden terminal prompt or stdin and save it in Keychain. Creates metadata when the label is new; updates the existing Keychain item when the environment name matches. <code>--global</code> also enables the mapping.")}
    ${command("secret forget", "synapse secret forget <vault.name>", "Delete the Keychain value, its metadata, and any global mapping.")}
    ${command("secret global", "synapse secret global <vault.name> <on|off>", "Enable or disable a secret’s global environment mapping. Enabling replaces the current global source for that environment name.")}
    ${command("scope init", "synapse scope init [folder] [--folder]", "Create a new <code>.synapse.yaml</code>. The folder defaults to the current directory. Use <code>--folder</code> for a folder-kind template; the command refuses when the file already exists.")}
    ${command("scope trust", "synapse scope trust [folder]", "Approve an exact scope path directly by parsing it, calculating its content digest, and storing that digest with its canonical path. <code>synapse allow</code> is the directory-oriented shortcut.")}
    ${command("scope status", "synapse scope status [folder] [--json]", "Alias the scope-oriented status flow to the same resolved output as <code>synapse status</code>.")}

    <h2 id="memory">Memory</h2>
    ${command("memory list", "synapse memory list [query] [--json]", "Search the body with the joined query words or list recent memory when empty. Returns up to 100 entries. Text mode prints ID, scope, source, and a compact preview.")}
    ${command("memory show", "synapse memory show <id> [--json]", "Print one exact memory with scope, project root, source, and timestamp, or return structured JSON.")}
    ${command("memory add", "synapse memory add [source] [--global|--project <folder>]", "Read a non-empty body from stdin. Project scope is the default and resolves from the current folder; use global only for context that belongs everywhere.")}
    ${command("memory edit", "synapse memory edit <id> [source]", "Read the replacement body from stdin and replace one existing memory. The optional source replaces the source label.")}
${command("memory import", "synapse memory import <claude|codex|markdown> [path] [--confirm]", "Preview recognized durable memory without changing its source. Add --confirm to import safe entries. Credential-shaped entries remain flagged unless the CLI also receives --include-flagged after source review.")}
${command("memory imports", "synapse memory imports [--json]", "List import batches, their provider, stored and linked counts, and whether each batch is active or undone.")}
${command("memory undo", "synapse memory undo <batch> --confirm", "Remove memories created only by that import batch. Preserve manually edited memories and records linked to another origin.")}
    ${command("memory delete", "synapse memory delete <id> --confirm", "Delete one memory. The exact <code>--confirm</code> guard is required.")}
    ${command("memory wipe", "synapse memory wipe --confirm", "Delete every memory entry and import batch while leaving SOUL.md, settings, vaults, Keychain values, and scope approvals intact. The exact guard is required.")}

    <h2 id="data">Data and settings</h2>
    ${command("data check", "synapse data check [--json]", "Open the database, run page and foreign-key integrity checks, apply supported migrations, and report the path, schema version, and <code>ok</code> integrity state.")}
    ${command("data export", "synapse data export <file>", "Create and validate a consistent SQLite snapshot at a destination that does not already exist. Secret values remain in Keychain and are not included.")}
    ${command("data restore", "synapse data restore <file>", "Validate a current-version snapshot and restore it while the app and MCP servers are closed. Preserves the previous database as a recovery backup and refuses without the exclusive lock.")}
    ${command("settings show", "synapse settings show", "Print the active recall optimization, result limit, character budget, supported shell modes, and zsh hook example.")}
    ${command("settings optimize", "synapse settings optimize <full|balanced|lean>", "Change the shared MCP recall response budget. Stored memory is not modified.")}
${command("guidance show", "synapse guidance show [--json]", "Print the SOUL.md path, whether it exists, pointer coverage, and whether both global instruction files are pointer-only.")}
${command("guidance sync", "synapse guidance sync", "Create SOUL.md when needed and refresh managed pointers in both global instruction files without removing unmanaged text.")}
${command("guidance adopt", "synapse guidance adopt --confirm", "Move unmanaged global guidance into SOUL.md, replace both global files with managed pointers, and retain backups.")}

    <h2 id="install">Installation and paths</h2>
    ${command("install", "synapse install", "Install the current executable for this user. A packaged app creates a launcher into the signed bundle; a development binary is copied atomically. Unrelated destination files are never overwritten.")}
    ${command("path", "synapse path", "Print the resolved data directory, SOUL.md, and CLI destination.")}
    ${command("version", "synapse version", "Print the application version. <code>--version</code> and <code>-V</code> are aliases.")}
    ${command("help", "synapse help", "Print the command summary. <code>--help</code> and <code>-h</code> are aliases at the top level.")}
  `,
};

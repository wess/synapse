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
    { label: "Agent mesh", id: "mesh" },
    { label: "Skills", id: "skills" },
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
    ${command("launch", "synapse launch &lt;claude|codex|pi&gt; [options] [-- &lt;flags&gt;]", "Start a coding tool with Synapse already in place: memory and the vault reachable over MCP, this folder's vault variables in its environment, and the project root it should treat as home. Everything after a bare <code>--</code> is passed to the tool untouched. A tool Synapse is not connected to is wired for the life of the process, so this works before setup has ever run and writes nothing into the tool's own configuration. Options: <code>--directory</code>, <code>--model</code>, <code>--allow-tool</code> (repeatable), <code>--strict</code>, <code>--no-vault</code>, <code>--skip-permissions</code>, <code>--as &lt;name&gt;</code> with <code>--role</code>, <code>--task</code>, and <code>--channel</code> to join the mesh as well, and <code>--print</code> to show the resolved command and environment names without running anything. Refuses on any scope warning, and never prints a secret value.")}
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

    <h2 id="mesh">Agent mesh</h2>
    <p>These commands need the mesh switched on with <code>synapse settings mesh on</code>. See the <a href="../mesh/">agent mesh guide</a> for what each part is for.</p>
    ${command("relay status", "synapse relay status [--json]", "Show whether the mesh is on, how many agents are reachable, and how many background workers are running.")}
    ${command("relay agents", "synapse relay agents [--json]", "List every agent with its role, reachability, last reported work state, and the project it is working in.")}
    ${command("relay channels", "synapse relay channels [--json]", "List the channels in use and how many agents subscribe to each.")}
    ${command("relay feed", "synapse relay feed [--follow] [--since &lt;id&gt;] [--json]", "Print the messages agents have sent each other. With <code>--follow</code> the command keeps printing new ones until interrupted.")}
    ${command("relay launch", "synapse relay launch &lt;name&gt; [options]", "Open one agent in this terminal, wired into the mesh with a role. Options: <code>--role</code>, <code>--tool claude|codex|pi</code>, <code>--task</code>, <code>--channel</code> (repeatable), <code>--allow-tool</code> (repeatable), <code>--model</code>, <code>--directory</code>, <code>--lead</code>, <code>--optimize</code>, <code>--strict</code>, <code>--skip-permissions</code>, <code>--command &lt;template&gt;</code>, and <code>--print</code> to show the resolved command without running it.")}
    ${command("relay team open", "synapse relay team open &lt;name&gt; [--directory &lt;folder&gt;]", "Open a whole roster. The first member runs in this terminal as the lead; the rest run in the background and stop when the lead closes.")}
    ${command("mux", "synapse mux [--as &lt;name&gt;] [--team &lt;team&gt;] [--channel &lt;name&gt;]... [--directory &lt;folder&gt;]", "Join the mesh as yourself and drive a team from one terminal. You get a name on the roster and the same messaging every agent has, so you can address any agent directly instead of relaying through a lead — and an agent that gets stuck can ask <em>you</em>. With <code>--team</code> the whole roster starts in the background with you as the lead. Type <code>@name text</code> for one agent, <code>#channel text</code> for a channel, <code>!text</code> for everyone, or a bare line to whoever is focused. <code>/help</code> lists the commands; <code>/quit</code> leaves and stops the workers it started. The name defaults to your login name.")}
    ${command("relay role", "synapse relay role &lt;list|show|create|edit|delete&gt; [name] [--user] [--json]", "Manage reusable agent roles. Create, edit, and delete write into the project by default, or into your own layer with <code>--user</code>. Editing a built-in copies it down first, and a file that does not parse is never saved.")}
    ${command("relay team", "synapse relay team &lt;list|show|create|edit|delete&gt; [name] [--user] [--json]", "Manage team rosters, resolved and edited exactly like roles.")}
    ${command("relay ps", "synapse relay ps [--json]", "List background workers with their state, process id, and log path.")}
    ${command("relay kill", "synapse relay kill &lt;name&gt;", "Stop a background worker. A worker owned by a Synapse session that is still running has to be stopped from there.")}
    ${command("session", "synapse session [--json]", "Report this session's Synapse connection as Claude Code session-hook output. Synapse installs this for you when you connect Claude Code; run it by hand to see exactly what a session will be told. Reads the calling tool's JSON on stdin.")}
    ${command("statusline", "synapse statusline", "Print one status line for a connected tool, reading that tool's JSON on stdin.")}

    <h2 id="skills">Skills</h2>
    <p>One Agent Skills library, installed into every connected tool. See the <a href="../skills/">skills guide</a> for the format and the folders involved.</p>
    ${command("skill list", "synapse skill list [--json]", "List the library with each skill's file count and description. A skill whose <code>SKILL.md</code> does not parse is reported on stderr and skipped.")}
    ${command("skill show", "synapse skill show &lt;name&gt;", "Print one skill's <code>SKILL.md</code>.")}
    ${command("skill create", "synapse skill create &lt;name&gt;", "Start a skill from a template. Names follow the standard: lowercase letters, digits, and single inner hyphens.")}
    ${command("skill edit", "synapse skill edit &lt;name&gt;", "Open a skill in <code>$VISUAL</code> or <code>$EDITOR</code>. A draft that does not parse is never saved over the working one.")}
    ${command("skill delete", "synapse skill delete &lt;name&gt; --confirm", "Remove a skill from the library. Copies already installed in tools are left alone.")}
    ${command("skill install", "synapse skill install [name] [--tool &lt;tool&gt;] [--replace]", "Copy the library into your tools. Without a name it installs everything; without <code>--tool</code> it installs into every connected tool. A copy that was edited in place, or a skill Synapse never wrote, is refused unless <code>--replace</code> is given.")}
    ${command("skill remove", "synapse skill remove &lt;name&gt; [--tool &lt;tool&gt;] [--force]", "Take a skill back out of a tool. Only a copy Synapse installed and nobody has changed is removed; <code>--force</code> overrides that.")}
    ${command("skill status", "synapse skill status [name] [--json]", "Show where each skill stands in each tool, and any skill a tool has that the library does not.")}
    ${command("skill adopt", "synapse skill adopt &lt;name&gt; [--tool &lt;tool&gt;]", "Copy a skill a tool already has into the library and record that tool as having it, so it stops reading as unmanaged.")}

    <h2 id="data">Data and settings</h2>
    ${command("data check", "synapse data check [--json]", "Open the database, run page and foreign-key integrity checks, apply supported migrations, and report the path, schema version, and <code>ok</code> integrity state.")}
    ${command("data export", "synapse data export <file>", "Create and validate a consistent SQLite snapshot at a destination that does not already exist. Secret values remain in Keychain and are not included.")}
    ${command("data restore", "synapse data restore <file>", "Validate a current-version snapshot and restore it while the app and MCP servers are closed. Preserves the previous database as a recovery backup and refuses without the exclusive lock.")}
    ${command("settings show", "synapse settings show", "Print the active recall optimization, result limit, character budget, supported shell modes, and zsh hook example.")}
    ${command("settings optimize", "synapse settings optimize <full|balanced|lean>", "Change the shared MCP recall response budget. Stored memory is not modified.")}
    ${command("settings mesh", "synapse settings mesh &lt;on|off&gt;", "Turn the agent mesh tools on or off. Connected tools pick the change up the next time they start.")}
${command("guidance show", "synapse guidance show [--json]", "Print the SOUL.md path, whether it exists, pointer coverage, and whether both global instruction files are pointer-only.")}
${command("guidance sync", "synapse guidance sync", "Create SOUL.md when needed and refresh managed pointers in both global instruction files without removing unmanaged text.")}
${command("guidance adopt", "synapse guidance adopt --confirm", "Move unmanaged global guidance into SOUL.md, replace both global files with managed pointers, and retain backups.")}

    <h2 id="tools">Tools Synapse does not ship</h2>
    <p>
      Codex, Claude Code, and pi are ordinary descriptors, not special cases. A
      descriptor is a TOML file saying where a tool keeps its files, what to run
      against its own CLI to connect it, how to read that back, and which flags
      it takes when Synapse starts it. Yours resolve from
      <code>.synapse/tools/</code> in a repository first, then your data
      directory, then the ones Synapse ships \u2014 so a project can carry the tool
      its team works in, and you can correct a built-in without waiting for a
      release. A described tool gets everything a built-in gets: connection,
      shared guidance, the skill library, and the mesh.
    </p>
    ${command("tool list", "synapse tool list [--json]", "Every tool this machine can connect to, with the layer each one resolves from.")}
    ${command("tool show", "synapse tool show &lt;name&gt;", "Print one descriptor and where it came from.")}
    ${command("tool create", "synapse tool create &lt;name&gt;", "Describe a tool Synapse does not ship. Opens a commented template in your editor and refuses to save a file that would not load. The name becomes the descriptor's file name and what you pass to <code>--tool</code>.")}
    ${command("tool edit", "synapse tool edit &lt;name&gt;", "Edit a descriptor. Editing one Synapse ships copies it into a layer you own first, so the shipped file stays as it was.")}
    ${command("tool delete", "synapse tool delete &lt;name&gt;", "Remove a descriptor you added. Deleting a copy that overrides a built-in returns you to the shipped one.")}

    <h2 id="install">Installation and paths</h2>
    ${command("install", "synapse install", "Install the current executable for this user. A packaged app creates a launcher into the signed bundle; a development binary is copied atomically. Unrelated destination files are never overwritten.")}
    ${command("path", "synapse path", "Print the resolved data directory, SOUL.md, and CLI destination.")}
    ${command("doctor", "synapse doctor [--json]", "Report everything a bug report needs: version, store state and size, connected tools and what each is set up with, skill and mesh state, shell and CLI integration, resolved paths, and recent crashes. Every check reports rather than fails, so a broken store is described instead of stopping the report. Nothing is sent anywhere.")}
    ${command("connect", "synapse connect [tool]", "Wire a tool into memory and the vault: register the Synapse MCP server through that tool's own CLI, and point its global instruction file at SOUL.md. With no name it connects every tool this machine has. Synapse never edits a tool's configuration itself \u2014 it asks the tool to.")}
    ${command("disconnect", "synapse disconnect [tool]", "Undo one tool's connection, or every tool's when no name is given: the MCP registration or installed package, the managed block in its instruction file, the Claude Code session notice and status line, and any skill Synapse installed for it. A skill you wrote, or a status line somebody else configured, is left alone.")}
    ${command("uninstall", "synapse uninstall [--data] [--confirm]", "Remove everything Synapse installed: every tool connection, the shell hook, and the command line tool. Without <code>--confirm</code> it prints what it would remove and stops. Your memory is left alone unless you also pass <code>--data</code>, which cannot be undone.")}
    ${command("version", "synapse version", "Print the application version. <code>--version</code> and <code>-V</code> are aliases.")}
    ${command("help", "synapse help", "Print the command summary. <code>--help</code> and <code>-h</code> are aliases at the top level.")}
  `,
};

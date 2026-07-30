import { note } from "../markup";
import type { Page } from "../types";

export const app: Page = {
  path: "docs/app/index.html",
  title: "Desktop app reference",
  description:
    "Use every Synapse screen with clear data boundaries, confirmation behavior, recovery paths, and shell-integration states.",
  kind: "docs",
  toc: [
    { label: "Navigation", id: "navigation" },
    { label: "Connections", id: "connections" },
    { label: "Memory", id: "memory" },
    { label: "Mesh", id: "mesh" },
    { label: "Vaults", id: "vaults" },
    { label: "Settings", id: "settings" },
    { label: "Editors and data", id: "editors" },
  ],
  body: `
    <h2 id="navigation">Navigation and shared state</h2>
    <p>The desktop app is a local control surface over the same SQLite database, Keychain items, scope files, and tool configuration used by the CLI and MCP server. The header moves between five screens:</p>
    <table>
      <thead><tr><th>Screen</th><th>Use it for</th><th>Material it can change</th></tr></thead>
      <tbody>
        <tr><td>Connections</td><td>Detect and connect supported developer tools.</td><td>Tool MCP configuration and managed instruction blocks.</td></tr>
        <tr><td>Memory</td><td>Import, scope, search, inspect, correct, or remove durable context.</td><td>Memory rows, scope metadata, and reversible import batches.</td></tr>
        <tr><td>Mesh</td><td>Turn the agent mesh on and watch who has joined, what they report, and what they send each other.</td><td>One local preference. The roster and messages are written by the agents themselves.</td></tr>
        <tr><td>Vaults</td><td>Manage labels, Keychain values, mappings, and approved project scopes.</td><td>Vault metadata, Keychain items, and <code>.synapse.yaml</code>.</td></tr>
        <tr><td>Settings</td><td>Manage shared guidance, recall, the agent mesh, appearance, CLI, and shell integration.</td><td><code>SOUL.md</code>, global pointers, local preferences, the CLI launcher, and one managed shell block.</td></tr>
      </tbody>
    </table>
    <p>Success and error notices appear inside the active screen. The app does not require an account and does not send this state to a hosted service.</p>

    <h2 id="connections">Connections</h2>
    <p>The Connections screen reports memory count, database size, and how many supported tools are connected. Each tool row has one of three states:</p>
    <dl>
      <dt>Not installed</dt>
      <dd>The command was not found on <code>PATH</code>. Set up is disabled until the tool is installed.</dd>
      <dt>Detected</dt>
      <dd>The tool executable is available but its exact <code>synapse</code> MCP entry is absent or stale. Choose <strong>Set up</strong>.</dd>
      <dt>Connected</dt>
      <dd>The stored executable and <code>["mcp"]</code> arguments match the expected Synapse server.</dd>
    </dl>
    <p>Set up registers the MCP server, creates <code>SOUL.md</code> when needed, and adds a marked pointer to that shared file in the tool's global instructions. For Claude Code it also installs the startup notice described below. Existing settings and text outside the managed block remain in place; changed files receive <code>.synapsebackup</code> siblings. If setup fails, Synapse restores the affected files.</p>
    <p>A connected <strong>Claude Code</strong> row also reports whether it announces Synapse at startup, with an <strong>Add</strong> or <strong>Remove</strong> control beside it. Adding it writes a SessionStart hook into that tool's own settings, so the connection is stated beside its welcome message before the model has written anything, and it claims the status line when nothing else has. A status line you configured yourself is never replaced — the row says <em>your status line kept</em> instead. Removing takes back only what Synapse wrote. Either way the change applies to the next session that tool starts, and it never requires disconnecting and reconnecting.</p>
    <p><strong>Edit instructions</strong> and <strong>Edit config</strong> open the actual files in the built-in editor. Synapse does not currently provide a separate Disconnect button; remove the named <code>synapse</code> entry and managed instruction block through those files when you intentionally want to disconnect a tool. See <a href="../config/#tools">Tool integration files</a> for their exact paths.</p>

    <h2 id="memory">Memory</h2>
    <p>The import panel previews Claude and Codex separately. <strong>Import safe</strong> stores recognized project memory, skips anything credential-shaped, leaves source files untouched, and records a reversible batch. <strong>Review source</strong> opens the provider folder. Undo requires confirmation and preserves imported records that were edited or linked from another source.</p>
    <p>An empty search shows recent memory; a query searches the stored body and shows up to 100 results. Select an entry to inspect its ID, local creation time, full Markdown body, source, and visibility. <strong>Global</strong> makes it available everywhere; <strong>Project</strong> requires a project root. <strong>Save changes</strong> replaces the selected body, source, and scope in place.</p>
    <p><strong>Delete</strong> changes to <strong>Confirm delete</strong> before removing one entry. <strong>Wipe memories</strong> separately changes to <strong>Confirm wipe</strong> before deleting the entire memory table. A wipe does not affect vault labels, Keychain values, scope approvals, or settings.</p>
    <p>Recall optimization changes responses, not what this screen stores or displays. Read <a href="../memory/">Memory and recall</a> for search behavior, response budgets, and CLI equivalents.</p>

    <h2 id="mesh">Mesh</h2>
    <p>The Mesh screen is off until you turn it on, here or in Settings. While it is off the screen explains the trade: the coordination tools are loaded by every connected tool, and that costs context in each session.</p>
    <p>Once on, it lists the agents that have joined with their role, project, and last reported work state; the background workers running under a Synapse session; and the recent messages between them. Nothing on this screen changes what agents do — it reports. <strong>Refresh</strong> re-reads the database, which is also what opening the screen does.</p>
    <p>See <a href="../mesh/">Agent mesh</a> for roles, teams, and the command line.</p>

    <h2 id="vaults">Vaults and scopes</h2>
    <p>Create a vault, select it, then provide a label, environment name, and value under <strong>Add a secret</strong>. <strong>Save to Keychain</strong> writes the value directly to macOS Keychain while SQLite keeps only its label, environment name, account reference, and scope state. The app never shows a saved value again.</p>
    <ul>
      <li><strong>Project only</strong> requires an approved YAML mapping; <strong>Global</strong> makes that environment name available without a project mapping.</li>
      <li><strong>Replace</strong> reads the current Secret value field and overwrites the selected Keychain item without changing its reference.</li>
      <li><strong>Forget</strong> requires <strong>Confirm</strong>, then removes both the Keychain item and its SQLite metadata.</li>
      <li><strong>Delete vault</strong> is available only when the selected vault is empty and requires a second confirmation.</li>
    </ul>
    <p>Under <strong>Project and folder scopes</strong>, choose a directory, create or edit its <code>.synapse.yaml</code>, inspect the reported state, then choose <strong>Approve</strong> only after reviewing the exact file. Any later edit invalidates that digest and requires another review. Secret values never enter YAML. See <a href="../vault/">Vaults and scopes</a> for resolution order and both process boundaries.</p>

    <h2 id="settings">Settings</h2>
    <h3>Shared guidance</h3>
    <p><strong>Open shared guidance</strong> edits <code>SOUL.md</code>. <strong>Sync pointers</strong> refreshes both global files without removing unmanaged content. <strong>Consolidate guidance</strong> requires confirmation, moves existing global text into the shared file, and leaves backups before making both global files pointer-only.</p>
    <h3>Recall optimization</h3>
    <p>Full, Balanced, and Lean change the shared MCP response limit and character budget. Original memory remains untouched. Balanced is the default; exact limits are documented in <a href="../memory/#budgets">Response budgets</a>.</p>
    <h3>Agent mesh</h3>
    <p>Off and On switch the coordination tools for every connected tool. Off keeps the tool list at its smallest, which is why it is the default. Tools already running keep the tool list they started with, so the change applies the next time each one starts. The same switch is on the <a href="#mesh">Mesh</a> screen.</p>
    <h3>Appearance</h3>
    <p>System follows the current macOS appearance as it changes. Light and Dark pin the app to that mode. The preference is stored locally.</p>
    <h3>Command line</h3>
    <p>The status is Installed, Not installed, or Conflict. <strong>Install CLI</strong> places the managed launcher at the displayed path. Synapse refuses to overwrite an unrelated executable; resolve a Conflict deliberately before trying again.</p>
    <h3>Shell environments</h3>
    <p>Command scoped always remains available through <code>synapse run -- &lt;command&gt;</code>. Automatic directory loading is an explicit opt-in for the detected default zsh, bash, or fish shell. Only that detected shell is changed.</p>
    <table>
      <thead><tr><th>Status</th><th>Control</th><th>Result</th></tr></thead>
      <tbody>
        <tr><td>Not enabled</td><td>Enable shell hook</td><td>Installs the CLI if needed and adds one marked startup-file block.</td></tr>
        <tr><td>Enabled</td><td>Remove hook</td><td>Removes only the marked block and leaves neighboring startup content intact.</td></tr>
        <tr><td>Needs repair</td><td>Repair hook or Remove</td><td>Replaces or removes only the changed managed block.</td></tr>
        <tr><td>Unavailable</td><td>Unavailable</td><td>No supported default shell or safe startup path could be detected; no file is changed.</td></tr>
      </tbody>
    </table>
    <p>Open a new terminal after enabling, repairing, or removing the hook. Existing terminals retain the integration they already loaded. Ambient values are readable by every child of an activated shell; use the command-scoped mode for a sensitive one-off process.</p>
    ${note("Startup files stay user-owned", "Synapse backs up and atomically rewrites the detected startup file, follows an existing symlink, and refuses malformed or duplicate managed markers instead of guessing what to replace.")}

    <h2 id="editors">Editors, local data, and recovery</h2>
    <p>The built-in editor handles <code>SOUL.md</code>, supported tool instructions, TOML or JSON configuration, and YAML scope files. Structured formats must validate before saving. Changed files are backed up and replaced atomically while existing permissions and symlinks are preserved.</p>
    <p>If an editor contains unsaved changes, Close and application quit are blocked until you choose Save or Discard. Saving a scope refreshes its state but does not approve it; review the result and choose Approve separately.</p>
    <p><strong>Open data folder</strong> on the Connections screen reveals the directory containing the local database. It does not create a backup and editing database files by hand is unsupported. Use <a href="../data/">Data lifecycle</a> for integrity checks, validated exports, exclusive restore, and recovery behavior.</p>
  `,
};

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
    { label: "MCP", id: "servers" },
    { label: "Mesh", id: "mesh" },
    { label: "Scenes and plugins", id: "scenes" },
    { label: "Skills", id: "skills" },
    { label: "Vaults", id: "vaults" },
    { label: "Settings", id: "settings" },
    { label: "Editors and data", id: "editors" },
  ],
  body: `
    <h2 id="navigation">Navigation and shared state</h2>
    <p>The desktop app is a local control surface over the same SQLite database, vault, scope files, and tool configuration used by the CLI and MCP server. The left column groups Map through Mesh under <strong>Workspace</strong>, installed extension surfaces such as Scenes under <strong>Plugins</strong>, and machine-wide Vaults and Settings under <strong>System</strong>.</p>
    <table>
      <thead><tr><th>Screen</th><th>Use it for</th><th>Material it can change</th></tr></thead>
      <tbody>
        <tr><td>Map</td><td>See the whole store at once: which projects it holds anything about, where the weight is, and what a memory sits next to.</td><td>Nothing. The map reads.</td></tr>
        <tr><td>Connections</td><td>Detect and connect supported developer tools.</td><td>Tool MCP configuration and managed instruction blocks.</td></tr>
        <tr><td>Stats</td><td>Measure recall use, context savings, and recent activity.</td><td>Nothing. Stats reads recorded local usage.</td></tr>
        <tr><td>Memory</td><td>Import, scope, search, inspect, correct, or remove durable context.</td><td>Memory rows, scope metadata, and reversible import batches.</td></tr>
        <tr><td>Skills</td><td>Keep one Agent Skills library, install it into every connected tool, and find skills other people published.</td><td>The library in the Synapse data directory, and skill folders inside each tool.</td></tr>
        <tr><td>MCP</td><td>Keep one list of MCP servers, give it to every connected tool, and find servers in the registry.</td><td>Server files in the Synapse data directory, their secrets in the vault, and each tool's MCP entries through its own CLI.</td></tr>
        <tr><td>Mesh</td><td>Turn the agent mesh on and watch who has joined, what they report, and what they send each other.</td><td>One local preference. The roster and messages are written by the agents themselves.</td></tr>
        <tr><td>Scenes</td><td>Watch live MCP sessions and choose an installed scene plugin.</td><td>The selected plugin, character choices, and animation preference.</td></tr>
        <tr><td>Vaults</td><td>Manage labels, stored values, mappings, the value store itself, and approved project scopes.</td><td>Vault metadata, stored values, and <code>.synapse.yaml</code>.</td></tr>
        <tr><td>Settings</td><td>Manage shared guidance, recall, the mesh, self-improvement, the worker limit, appearance, CLI, and shell integration.</td><td><code>SOUL.md</code>, global pointers, local preferences, the CLI launcher, and one managed shell block.</td></tr>
      </tbody>
    </table>
    <p>Success and error notices appear inside the active screen. The app does not require an account and does not send this state to a hosted service.</p>

    <h2 id="map">Map</h2>
    <p>Every other screen is a list, and a list answers what is stored rather than what it is about. The Map draws the store as a graph: one circle per memory, one larger circle per project, and lines between them. Projects come out as clusters, a correction comes out as a line to the thing it corrected, and a memory nothing else touches comes out looking like one — usually either the most interesting memory on the map or the one that should not be there.</p>
    <p>A point is sized by how much is tied to it and coloured by the project it belongs to. Click one to read that memory beside the map, with its scope, source, and date. A hollow point is a superseded memory: out of recall, still in the store, still restorable.</p>
    <p>The map is laid out in three dimensions, and the window lets you move around inside it:</p>
    <table>
      <thead><tr><th>Do this</th><th>Get this</th></tr></thead>
      <tbody>
        <tr><td>Drag</td><td>Turn the cloud. Nearer points are drawn larger and brighter.</td></tr>
        <tr><td>Scroll</td><td>Closer or further out, toward whatever the pointer is over.</td></tr>
        <tr><td>Shift-drag</td><td>Slide the cloud within the frame.</td></tr>
        <tr><td>Click a point</td><td>Read that memory beside the map.</td></tr>
        <tr><td>Reset view</td><td>Put the camera back where it started. It appears once you have moved it.</td></tr>
      </tbody>
    </table>
    <p>A plane is the wrong shape for a graph: a cluster with more neighbours than the plane has room for either overlaps itself or pushes everything else off the map, and both of those are the picture lying about the store. Turning it re-draws, never re-lays-out, so the shape you learned stays the shape it is.</p>
    <p>The plot is dark whatever theme you use. A glow is light added to what is behind it, and on a light background there is nothing to add to; the rest of the window stays in your theme.</p>
    <p>Three kinds of line, and only one of them is inferred:</p>
    <dl>
      <dt>A memory and its project</dt>
      <dd>Recorded. Every memory hangs off the project it was stored for, or off global memory.</dd>
      <dt>A memory and the one that replaced it</dt>
      <dd>Recorded, and drawn in red. This is what supersession looks like from above.</dd>
      <dt>Two memories using the same uncommon words</dt>
      <dd>Inferred, and drawn faintest. It uses the same word list recall uses, so a search and a map cannot disagree about which words mean nothing. It is a resemblance, never a claim that two memories say the same thing.</dd>
    </dl>
    <p>The map is bounded on purpose: it draws the newest 160 memories and says so whenever the store is larger. The layout is the same every time you open it, because a map is worth having when you have learned its shape.</p>
    ${note("The map only reads", "Nothing on this screen changes anything. It opens the store when you arrive and when you press Refresh, and holds nothing between them.")}

    <h2 id="connections">Connections</h2>
    <p>The Connections screen reports memory count, database size, and how many supported tools are connected. It draws two lists: <strong>Connected</strong>, the tools wired in right now, above <strong>Supported</strong>, everything else Synapse can connect on this machine. A tool appears in one or the other, never both, and moves between them as you connect and disconnect it.</p>
    <p>Each tool row has one of four states:</p>
    <dl>
      <dt>Not installed</dt>
      <dd>The command was not found on <code>PATH</code>. Set up is disabled until the tool is installed.</dd>
      <dt>Detected</dt>
      <dd>The tool executable is available but its exact <code>synapse</code> MCP entry is absent or stale. Choose <strong>Set up</strong>.</dd>
      <dt>Connected</dt>
      <dd>The stored executable and <code>["mcp"]</code> arguments match the expected Synapse server.</dd>
      <dt>Update available</dt>
      <dd>Connected, and this release would connect it differently — its descriptor changed after the connection was made. Nothing is broken; there is a newer answer to apply. Choose <strong>Update</strong>.</dd>
    </dl>
    <p>Set up registers the MCP server, creates <code>SOUL.md</code> when needed, and adds a marked pointer to that shared file in the tool's global instructions. For Claude Code it also installs the startup notice described below. Existing settings and text outside the managed block remain in place; changed files receive <code>.synapsebackup</code> siblings. If setup fails, Synapse restores the affected files.</p>
    <p>A connected <strong>Claude Code</strong> row also reports whether it announces Synapse at startup, with an <strong>Add</strong> or <strong>Remove</strong> control beside it. Adding it writes two hooks into that tool's own settings: a SessionStart hook, so the connection is stated beside its welcome message before the model has written anything, and a PreCompact hook that asks a long session to write down what it settled before its context is compacted away. The SessionStart hook also runs after a compaction, without the notice, to hand the session its memory back. A hook installed by an earlier release is brought up to date the next time the tool starts the MCP server. It also claims the status line when nothing else has. A status line you configured yourself is never replaced — the row says <em>your status line kept</em> instead. Removing takes back only what Synapse wrote. Either way the change applies to the next session that tool starts, and it never requires disconnecting and reconnecting.</p>
    <h3 id="maintaining">Keeping a connection current</h3>
    <p>A connected row carries three controls, in increasing order of what they cost you:</p>
    <dl>
      <dt>Update</dt>
      <dd>Applies this release's descriptor to a tool that is already connected. The registration is written again rather than skipped, because detection cannot tell a descriptor that moved from one that did not — it only knows an entry is present and points at Synapse. Nothing is removed, your skills stay installed, and the tool is connected throughout. This is what the <em>Update available</em> badge is asking for.</dd>
      <dt>Reset</dt>
      <dd>Disconnects the tool and connects it again. The bigger hammer, and the one that costs something: disconnecting removes the skills Synapse installed for that tool, and a reset puts the connection back rather than the library. Reach for Update first; use Reset when what is on disk has to go before the new answer can land.</dd>
      <dt>Remove</dt>
      <dd>Disconnects and leaves it that way: the MCP registration, the managed instruction block, the Claude Code hooks and status line, and any skill Synapse installed for it. A skill you wrote, your own words in an instruction file, and a status line somebody else configured all stay where they are. The row moves down to <strong>Supported</strong>.</dd>
    </dl>
    <p>Synapse knows a descriptor moved because it records which one a connection was made from. A tool connected before this release has no such record, and reads as current rather than out of date — not knowing is not the same as knowing something is stale.</p>
    <p><strong>Edit instructions</strong> and <strong>Edit config</strong> open the actual files in the built-in editor. See <a href="../config/#tools">Tool integration files</a> for their exact paths.</p>

    <h2 id="memory">Memory</h2>
    <p>The import panel previews Claude and Codex separately. <strong>Import safe</strong> stores recognized project memory, skips anything credential-shaped, leaves source files untouched, and records a reversible batch. <strong>Review source</strong> opens the provider folder. Undo requires confirmation and preserves imported records that were edited or linked from another source.</p>
    <p>An empty search shows recent memory; a query searches the stored body and source and shows up to 100 results. Select an entry to inspect its ID, local creation time, full Markdown body, source, and visibility. <strong>Global</strong> makes it available everywhere; <strong>Project</strong> needs a project. The <strong>Project</strong> field shows the key the memory is filed under: a repository's remote such as <code>github.com/owner/repo</code>, or a folder path for a project without one. Leave it as it is to keep the memory where it is, or enter a folder path or another key to move it. <strong>Save changes</strong> replaces the selected body, source, and scope in place.</p>
    <p><strong>Delete</strong> changes to <strong>Confirm delete</strong> before removing one entry. <strong>Wipe memories</strong> separately changes to <strong>Confirm wipe</strong> before deleting the entire memory table. A wipe does not affect vault labels, stored values, scope approvals, or settings.</p>
    <p>Recall optimization changes responses, not what this screen stores or displays. Read <a href="../memory/">Memory and recall</a> for search behavior, response budgets, and CLI equivalents.</p>

    <h2 id="servers">MCP</h2>
    <p>The MCP screen has the same two tabs as Skills. <strong>Installed</strong> is a card per library server with the command it runs and a badge per tool. A server that still needs a secret says so with a <strong>Set</strong> button beside each one, and the value goes from that field into the vault; <strong>Install</strong> stays off until nothing is missing. <strong>Edit</strong> opens the server's file, and a draft that is not a valid server is not saved. Servers your tools already have appear under <strong>Already in your tools</strong> with <strong>Adopt</strong>, and an adopted server offers <strong>Swap</strong> to replace that tool's own entry, plaintext and all, with the library's.</p>
    <p><strong>Search</strong> asks the official MCP Registry, which can take half a minute. Selecting a result shows the exact command it would become; <strong>Add to library</strong> keeps it without touching any tool.</p>
    <p>See <a href="../servers/">MCP servers</a> for the file format, the states, and what is and is not kept.</p>

    <h2 id="mesh">Mesh</h2>
    <p>The Mesh screen is off until you turn it on, here or in Settings. While it is off the screen explains the trade: the coordination tools are loaded by every connected tool, and that costs context in each session.</p>
    <p>Once on, supervised workers lead the screen and are grouped into working and needs-attention states. The side roster shows other agents that are online now; offline and expected rows stay collapsed until you ask for them. Recent messages remain below the live roster. Nothing on this screen changes what agents do — it reports. <strong>Refresh</strong> re-reads the database, which is also what opening the screen does.</p>
    <p>See <a href="../mesh/">Agent mesh</a> for roles, teams, and the command line.</p>

    <h2 id="scenes">Scenes and plugins</h2>
    <p>Scenes appears under <strong>Plugins</strong> in the sidebar. It shows each live MCP transport as a room, even when that session has not joined the mesh. Select a room for its harness, project, heartbeat, and reported state; choose a character, filter by project, or pause animation without stopping presence updates.</p>
    <p><strong>Open plugins folder</strong> reveals the local install location and <strong>Reload plugins</strong> discovers changes without restarting the app. For a safer development loop, use <code>synapse plugin init</code>, <code>check</code>, and <code>install</code>; see <a href="../scenes/">Scenes and plugins</a> for the manifest, asset bounds, and replacement backup.</p>

    <h2 id="skills">Skills</h2>
    <p>The Skills screen lists your library, and beside each skill a badge per connected tool saying whether that tool has it, has an older copy, or has one Synapse did not write. <strong>Install</strong> on a skill copies it wherever it is missing or behind; <strong>Install all</strong> does the lot.</p>
    <p>Skills a tool already has that the library does not know about appear under <strong>Already in your tools</strong> with an <strong>Adopt</strong> button, which copies one into the library and starts managing it. A skill Synapse did not install is never overwritten or deleted from this screen — it is reported instead, and the result line says how many were left alone.</p>
    <p>The <strong>Search</strong> tab asks skills.sh. Results are listed down the left; selecting one reads its whole <code>SKILL.md</code> into the panel beside them without keeping anything, and <strong>Add to library</strong> brings it in waiting for review, so it shows up on the Installed tab with Approve and Turn down rather than in any tool.</p>
    <p>See <a href="../skills/">Skills</a> for the format, the exact folders, and what each state means.</p>

    <h2 id="vaults">Vaults and scopes</h2>
    <p>Create a vault, select it, then provide a label, environment name, and value under <strong>Add a secret</strong>. <strong>Save to the vault</strong> writes the value straight into the value store while <code>brain.db</code> keeps only its label, environment name, account reference, and scope state. The app never shows a saved value again.</p>
    <ul>
      <li><strong>Project only</strong> requires an approved YAML mapping; <strong>Global</strong> makes that environment name available without a project mapping.</li>
      <li><strong>Copy</strong> puts one value on the clipboard. It is the only way a stored value comes back out, and it never passes through the window.</li>
      <li><strong>Replace</strong> reads the current Secret value field and overwrites the stored value without changing its reference.</li>
      <li><strong>Forget</strong> requires <strong>Confirm</strong>, then removes the stored value and its metadata.</li>
      <li><strong>Delete vault</strong> is available only when the selected vault is empty and requires a second confirmation.</li>
    </ul>
    <p>A badge beside the counts names the store this machine keeps values in — <strong>Encrypted vault.db</strong> or <strong>macOS Keychain</strong> — and the button next to it moves every value into the other one. That takes two clicks: the first asks, the second migrates. See <a href="../vault/#backend">Where values live</a> for what the two stores protect.</p>
    <p>Under <strong>Project and folder scopes</strong>, choose a directory, create or edit its <code>.synapse.yaml</code>, inspect the reported state, then choose <strong>Approve</strong> only after reviewing the exact file. Any later edit invalidates that digest and requires another review. Secret values never enter YAML. See <a href="../vault/">Vaults and scopes</a> for resolution order and both process boundaries.</p>

    <h2 id="settings">Settings</h2>
    <h3>Shared guidance</h3>
    <p><strong>Open shared guidance</strong> edits <code>SOUL.md</code>. <strong>Sync pointers</strong> refreshes both global files without removing unmanaged content. <strong>Consolidate guidance</strong> requires confirmation, moves existing global text into the shared file, and leaves backups before making both global files pointer-only.</p>
    <h3>Recall optimization</h3>
    <p>Full, Balanced, and Lean change the shared MCP response limit and character budget. Original memory remains untouched. Balanced is the default; exact limits are documented in <a href="../memory/#budgets">Response budgets</a>.</p>
    <h3>Agent mesh</h3>
    <p>Off and On switch the coordination tools for every connected tool. Off keeps the tool list at its smallest, which is why it is the default. Tools already running keep the tool list they started with, so the change applies the next time each one starts. The same switch is on the <a href="#mesh">Mesh</a> screen.</p>
    <h3>Self-improvement</h3>
    <p>Off and On control <code>teach</code>, <code>revise</code>, <code>feedback</code>, and <code>resolvedrift</code>. Off is the default. With learning enabled, sessions collect outcome evidence and suggest procedures during ordinary work. New skills and replacement instructions wait for approval; collection, review, and evaluation also work through the CLI without opening the app. See <a href="../skills/#learning">Learning</a>.</p>
    <h3>Background workers</h3>
    <p>The most agents one supervisor may run at once. Every worker is a separate session on the account you already pay for, so this is a spending limit as much as a performance one. Whatever you choose is clamped to a ceiling in code, so a mistyped number cannot buy an unbounded fleet, and <code>synapse settings workers &lt;count&gt;</code> takes any value up to it. A supervisor already running picks a change up on its next spawn.</p>
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

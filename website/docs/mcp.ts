import { code, note } from "../markup";
import type { Page } from "../types";

export const mcp: Page = {
  path: "docs/mcp/index.html",
  title: "MCP tool reference",
  description: "Run the local stdio server and understand the exact request, response, and value boundary of every exposed tool.",
  kind: "docs",
  toc: [
    { label: "Server", id: "server" },
    { label: "Session start", id: "sessionstart" },
    { label: "Before compaction", id: "compaction" },
    { label: "remember", id: "remember" },
    { label: "recall", id: "recall" },
    { label: "vaultstatus", id: "vaultstatus" },
    { label: "teach and revise", id: "learning" },
    { label: "Mesh tools", id: "mesh" },
    { label: "Operational behavior", id: "behavior" },
  ],
  body: `
    <h2 id="server">Server</h2>
    <p>Synapse implements an MCP stdio server. A tool launches the same signed or installed executable with the <code>mcp</code> argument:</p>
    ${code("shell", `~/.local/bin/synapse mcp`)}
    <p>The process uses stdin and stdout for protocol messages. Do not wrap it in a command that writes banners or shell setup output to stdout. The server opens the same local database as the desktop app and holds a shared lifecycle lock while connected.</p>

    <h2 id="sessionstart">Session start</h2>
    <p>Connecting Claude Code also installs a <code>SessionStart</code> hook that runs <code>synapse session</code>. It does two things a tool cannot do for itself: it prints a line in the terminal before the model has written anything, and it puts this project's memory into the session's context before the first turn. A connected pi runs the same command from its extension and shows the same two halves.</p>
    ${code("shell", `synapse session --json`)}
    ${code("json", `{
  "systemMessage": "Synapse connected · 128 memories",
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Synapse is connected and holds 128 memories for /Users/example/project. …"
  }
}`)}
    <p><code>additionalContext</code> carries the memories themselves, most recent first, under the same scope rule <code>recall</code> uses: everything global, plus everything stored for this project, and nothing from any other project. A session therefore opens already holding what the project has decided, rather than being asked to go and look.</p>
    <p>That matters because the alternative was guidance. Asking a model to call <code>recall</code> before it starts is an instruction it may or may not follow, and a session that skipped it worked from nothing while still reporting a connection. Guidance still asks for <code>recall</code>, because a focused query in the middle of a task is the case a session-start recall cannot cover.</p>
    <table>
      <thead><tr><th>Behavior</th><th>Detail</th></tr></thead>
      <tbody>
        <tr><td>Budget</td><td>Recalls under a <code>balanced</code> ceiling. A per-call budget can only shrink your configured one, so a store set to <code>lean</code> still returns <code>lean</code>.</td></tr>
        <tr><td>Scope</td><td>Global memory plus the project the calling tool reports through <code>cwd</code> or <code>workspace.current_dir</code>, falling back to <code>SYNAPSE_PROJECT_DIR</code>.</td></tr>
        <tr><td>Empty store</td><td>No block is injected. The context asks the tool to call <code>remember</code> once something durable is settled.</td></tr>
        <tr><td>Failure</td><td>Reports <code>Synapse unavailable</code> with a short reason and tells the model not to claim a connection that is not there. The hook never exits non-zero, because a failing hook is noise in your terminal.</td></tr>
        <tr><td>Trust</td><td>Recalled content is labelled as context, never as instruction. It does not override the current request, repository guidance, or what you say next.</td></tr>
      </tbody>
    </table>
    ${note("Codex has no equivalent hook", "Codex does not expose a session-start hook, so a Codex session still opens by calling recall itself as the shared guidance in SOUL.md asks. Everything else — the tools, the scope rule, the response budget — is identical.")}
    ${note("pi reaches the server through a package", "pi has no MCP client, so its connection is the synapse-pi package. The extension in it starts the same server, registers whatever tools that server advertises, runs the same session-start recall, and shows the same status line. Turn the mesh on and its sixteen tools appear in pi too, on the next start.")}

    <h2 id="compaction">Before compaction</h2>
    <p>The other end of the same session. When a long session is about to be compacted, everything it worked out that nobody wrote down is about to stop existing \u2014 and it is the only moment where not having written something down costs immediately. Connecting Claude Code installs a <code>PreCompact</code> hook that runs <code>synapse compact</code>; a connected pi runs the same command from <code>session_before_compact</code>.</p>
    ${code("shell", `synapse compact`)}
    ${code("json", `{
  "hookSpecificOutput": {
    "hookEventName": "PreCompact",
    "additionalContext": "Synapse holds this project's durable memory, and this session is about to be compacted\u2026"
  }
}`)}
    <p>It asks for an explicit list of what the session settled and is not already stored, and for a <code>remember</code> call for each one. It deliberately recalls <em>nothing</em>: the context window is being reclaimed, and spending it re-injecting memory the session already had is the opposite of what a compaction is for.</p>
    <p>The compaction itself is never blocked, cancelled, or rewritten. A memory tool that traded a session's whole context for a reminder would be a worse bargain than forgetting.</p>

    <h2 id="remember">remember</h2>
    <p>Stores a durable fact, decision, preference, convention, or correction.</p>
    ${code("json", `{
  "content": "Use small focused modules.",
  "source": "synapse",
  "scope": "project",
  "project": "/Users/example/project",
  "supersedes": [18]
}`)}
    <table>
      <thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td><code>content</code></td><td>string</td><td>Yes</td><td>The durable text. Empty or whitespace-only content is rejected.</td></tr>
        <tr><td><code>source</code></td><td>string or null</td><td>No</td><td>An origin such as a project path, repository name, or topic.</td></tr>
        <tr><td><code>scope</code></td><td><code>project</code> or <code>global</code></td><td>No</td><td>Defaults to project. Use global only for context that should appear everywhere.</td></tr>
        <tr><td><code>project</code></td><td>string or null</td><td>For project scope</td><td>Absolute working-project path. Synapse normalizes nested paths to the project root.</td></tr>
        <tr><td><code>supersedes</code></td><td>array of integers or null</td><td>No</td><td>Ids of memories this one replaces \u2014 usually ids that came back from an earlier <code>recall</code>. Each stops being recalled. Nothing is deleted. An id that no longer resolves is skipped rather than failing the write.</td></tr>
      </tbody>
    </table>
    ${code("json", `{
  "id": 24,
  "stored": true,
  "superseded": [18]
}`)}
    <p>The store and the replacement happen in one transaction. Without <code>supersedes</code>, a correction is a second memory contradicting the first, both come back from the next recall, and the ranking decides which one an agent acts on \u2014 including the version its own author had already retracted.</p>

    <h2 id="recall">recall</h2>
    <p>Returns durable context relevant to a query. An empty query returns recent memory.</p>
    ${code("json", `{
  "query": "module structure",
  "limit": 8,
  "budget": "lean",
  "project": "/Users/example/project"
}`)}
    <table>
      <thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td><code>query</code></td><td>string</td><td>Yes</td><td>Words or a phrase describing the context needed. Use an empty string for recent entries.</td></tr>
        <tr><td><code>limit</code></td><td>integer or null</td><td>No</td><td>Requested result count. Defaults to 8. The active response budget may lower it.</td></tr>
        <tr><td><code>budget</code></td><td><code>full</code>, <code>balanced</code>, or <code>lean</code></td><td>No</td><td>May reduce the configured response ceiling but can never enlarge it.</td></tr>
        <tr><td><code>project</code></td><td>string or null</td><td>No</td><td>Absolute working-project path. Results include global memory plus this project and exclude other projects.</td></tr>
      </tbody>
    </table>
    ${code("json", `{
  "optimization": "lean",
  "memories": [
    {
      "id": 24,
      "body": "Use small focused modules.",
      "source": "synapse",
      "scope": "project",
      "project": "/Users/example/project",
      "created": 1785250000
    }
  ]
}`)}
    <p>Two fields appear only when they are true of the result. <code>abridged</code> marks a memory returned as its opening sentence alone, because the response budget could not carry the rest \u2014 recall it again with a narrower query or a larger budget before acting on the part you cannot see. <code>superseded</code> never appears here at all, because a replaced memory is not recalled.</p>

    <h2 id="vaultstatus">vaultstatus</h2>
    <p>Lists active environment-variable names and scope trust state for a folder. It never returns secret values and cannot inject them into the connected tool.</p>
    ${code("json", `{
  "path": "/Users/example/project"
}`)}
    <p><code>path</code> is optional. Resolution falls back to <code>SYNAPSE_PROJECT_DIR</code>, then the server process’s current directory.</p>
    ${code("json", `{
  "path": "/Users/example/project",
  "available": ["DATABASE_URL"],
  "ambient": "ready",
  "shell": "zsh",
  "scopes": [
    {
      "path": "/Users/example/project/.synapse.yaml",
      "scope": "project",
      "trusted": true,
      "changed": false,
      "env": ["DATABASE_URL"],
      "denied": [],
      "error": null
    }
  ],
  "warnings": [],
  "note": "Values stay in Keychain. Use synapse run for one child or install the shell hook for an approved directory."
}`)}
    ${note("Metadata, not a secret channel", "The names in available tell a tool what a scoped command or activated shell could receive. vaultstatus never reads the corresponding Keychain values and cannot change a connected tool’s environment.")}

    <h2 id="learning">teach and revise</h2>
    <p>Two tools that appear only while <code>synapse settings learn on</code> is set. They are how a session writes down a procedure it worked out, and corrects one that turned out wrong. Both write to the Synapse skill library and to nothing else.</p>
    <p><code>teach</code> takes a <code>name</code>, a one-line <code>description</code>, the <code>instructions</code> as Markdown, a <code>scope</code> of <code>project</code> (the default) or <code>global</code>, the absolute <code>project</code> root, and a <code>note</code> for you saying why it was worth keeping. Synapse writes the frontmatter itself rather than accepting it, so a model never gets to invent YAML keys or a name that disagrees with its own directory. The result always says the same thing: stored, and waiting for you.</p>
    <p><code>revise</code> takes the <code>name</code>, the corrected <code>instructions</code> in full, and a <code>note</code> saying what was wrong with the version it replaces. It returns the revision id holding the old text and the tools the correction reached.</p>
    ${note("Teaching is free; installing is the decision", "A taught skill is in the library and in no tool until you approve it, so a session can never change how the next one behaves. A revision is the exception and reaches the copies Synapse installed \u2014 you already agreed to that skill being loaded, and a correction that never arrives leaves every session running the version that was wrong. What it said before is kept either way. See the <a href=\"../skills/#learning\">skills guide</a>.")}

    <h2 id="mesh">Mesh tools</h2>
    <p>Three tools are always present. Sixteen more appear only while the agent mesh is switched on, and two more while self-improvement is, because a tool definition costs context in every session that loads it. They let connected sessions register under a name, message each other directly or by channel, park on <code>wait</code> until work arrives, report and watch work state, and start or stop background workers.</p>
    <p>The guidance explaining them is sent with them and withdrawn with them, so the tool list and the instructions can never be out of step. That is true of both switches. See the <a href="../mesh/">agent mesh guide</a> for the full list and what each one is for.</p>

    <h2 id="behavior">Operational behavior</h2>
    <ul>
      <li>Tool errors are returned as readable strings. Protocol messages remain on stdio.</li>
      <li>The server identifies itself as <code>synapse</code> using the application version and advertises tool capability only.</li>
      <li>Opening the database runs integrity and relationship checks, applies numbered migrations, and secures database permissions.</li>
      <li>A running MCP process holds a shared database lock. <code>synapse data restore</code> requires an exclusive lock and therefore refuses while any connected server or the desktop app is using the database.</li>
      <li>Connected tools should recall before decisions that depend on project history and remember only stable confirmed context after it is established. A Claude Code session starts with that memory already in context; see <a href="#sessionstart">Session start</a>.</li>
    </ul>
  `,
};

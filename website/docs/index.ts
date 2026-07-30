import type { Page } from "../types";

export const overview: Page = {
  path: "docs/index.html",
  title: "Start here",
  description: "Understand the Synapse model, choose a path through the guide, and know where its guarantees stop.",
  kind: "docs",
  toc: [
    { label: "The model", id: "model" },
    { label: "What Synapse stores", id: "stores" },
    { label: "What Synapse does not do", id: "limits" },
    { label: "Choose a path", id: "paths" },
  ],
  body: `
    <h2 id="model">The model</h2>
    <p>Synapse is a local service with two deliberately separate jobs. Its <strong>memory layer</strong> gives connected developer tools one durable, searchable context store. Its <strong>vault layer</strong> gives commands or opted-in shells carefully scoped environment variables while keeping secret values in macOS Keychain.</p>
    <p>The desktop app is the control surface. The CLI exposes the same memory, vault, scope, settings, and data-lifecycle operations. The MCP stdio server gives connected tools three narrow capabilities: remember durable context, recall it, and inspect value-free vault status. An optional <strong>agent mesh</strong> adds a fourth job on top of the same store: letting those connected tools coordinate with each other.</p>

    <h2 id="stores">What Synapse stores</h2>
    <table>
      <thead><tr><th>Material</th><th>Location</th><th>Visible to</th></tr></thead>
      <tbody>
        <tr><td>Memory text, source, global or project scope, origin, and import history</td><td>Local SQLite database</td><td>Desktop app, CLI, and scoped MCP memory tools</td></tr>
        <tr><td>Shared working guidance</td><td><code>SOUL.md</code> in the Synapse data directory</td><td>You and every connected tool through managed global pointers</td></tr>
        <tr><td>Vault names, secret labels, Keychain account references, global mappings</td><td>Local SQLite database</td><td>Desktop app and CLI; MCP receives names only</td></tr>
        <tr><td>Secret values</td><td>macOS Keychain</td><td>Synapse, a child launched with <code>synapse run</code>, or processes launched from an activated shell</td></tr>
        <tr><td>Project and folder mappings</td><td>Approved <code>.synapse.yaml</code> files</td><td>You, Synapse, and the repository if you commit the file</td></tr>
        <tr><td>Scope approvals</td><td>Local SQLite database as a path and content digest</td><td>Synapse</td></tr>
        <tr><td>Mesh roster, channels, and messages between agents</td><td>Local SQLite database, while the mesh is on</td><td>Desktop app, CLI, and agents that have joined</td></tr>
        <tr><td>Agent Skills library</td><td>The Synapse data directory, copied into each tool's own skills folder</td><td>You, and every tool that reads the Agent Skills format</td></tr>
        <tr><td>Agent roles and team rosters</td><td>TOML in the project's <code>.synapse</code> folder or the Synapse data directory</td><td>You, Synapse, and the repository if you commit them</td></tr>
      </tbody>
    </table>

    <h2 id="limits">What Synapse does not do</h2>
    <ul>
      <li>It does not sync memory or secrets to an account or hosted service.</li>
      <li>It does not send secret values through MCP, write them to YAML, or accept them as command arguments.</li>
      <li>It cannot directly modify an already-running parent process. Use <code>synapse run -- &lt;command&gt;</code> for one child, or explicitly evaluate <code>synapse hook</code> so your shell applies quoted environment changes itself.</li>
      <li>It does not decide that every conversation detail deserves permanent memory. Connected tools receive instructions to keep stable, confirmed context.</li>
      <li>It does not continuously mirror native tool stores. Imports are explicit, previewed, and reversible; new shared memory belongs in Synapse.</li>
      <li>The current beta is a signed Apple-silicon build for macOS 13 or later.</li>
    </ul>

    <h2 id="paths">Choose a path</h2>
    <dl>
      <dt>I want to use Synapse now.</dt>
      <dd>Follow <a href="../tutorials/connect/">Install and connect your first tools</a>. It covers download, first launch, the CLI, both supported tools, and a working recall check.</dd>
      <dt>I want to manage Synapse from the app.</dt>
      <dd>Use the <a href="app/">desktop app reference</a> for every screen, status, confirmation, editor, and shell-integration control.</dd>
      <dt>I want to understand memory behavior.</dt>
      <dd>Read <a href="memory/">Memory and recall</a>, then follow <a href="../tutorials/continuity/">Carry one decision between tools</a>.</dd>
      <dt>I need credentials for local commands.</dt>
      <dd>Read <a href="vault/">Vaults and scopes</a>, then build a complete approved scope in <a href="../tutorials/secrets/">Use a scoped secret in either shell mode</a>.</dd>
      <dt>I want my tools to work together.</dt>
      <dd>Read <a href="mesh/">Agent mesh</a>. It covers turning it on, roles, teams, background workers, and what the mesh deliberately does not do.</dd>
      <dt>I keep copying the same skill into every tool.</dt>
      <dd>Read <a href="skills/">Skills</a>. One library, installed into Claude Code and Codex together, with drift reported rather than silently resolved.</dd>
      <dt>I need automation or exact syntax.</dt>
      <dd>Use the <a href="cli/">complete CLI reference</a> and <a href="mcp/">MCP tool reference</a>.</dd>
      <dt>I am planning backups or a recovery.</dt>
      <dd>Read <a href="data/">Data lifecycle</a> before exporting or restoring the database.</dd>
    </dl>
  `,
};

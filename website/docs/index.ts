import type { Page } from "../types";

export const overview: Page = {
  path: "docs/index.html",
  title: "Start here",
  description: "Understand the Synaps model, choose a path through the guide, and know where its guarantees stop.",
  kind: "docs",
  toc: [
    { label: "The model", id: "model" },
    { label: "What Synaps stores", id: "stores" },
    { label: "What Synaps does not do", id: "limits" },
    { label: "Choose a path", id: "paths" },
  ],
  body: `
    <h2 id="model">The model</h2>
    <p>Synaps is a local service with two deliberately separate jobs. Its <strong>memory layer</strong> gives connected developer tools one durable, searchable context store. Its <strong>vault layer</strong> gives commands or opted-in shells carefully scoped environment variables while keeping secret values in macOS Keychain.</p>
    <p>The desktop app is the control surface. The CLI exposes the same memory, vault, scope, settings, and data-lifecycle operations. The MCP stdio server gives connected tools three narrow capabilities: remember durable context, recall it, and inspect value-free vault status.</p>

    <h2 id="stores">What Synaps stores</h2>
    <table>
      <thead><tr><th>Material</th><th>Location</th><th>Visible to</th></tr></thead>
      <tbody>
        <tr><td>Memory text, source, and timestamp</td><td>Local SQLite database</td><td>Desktop app, CLI, and MCP memory tools</td></tr>
        <tr><td>Vault names, secret labels, Keychain account references, global mappings</td><td>Local SQLite database</td><td>Desktop app and CLI; MCP receives names only</td></tr>
        <tr><td>Secret values</td><td>macOS Keychain</td><td>Synaps, a child launched with <code>synaps run</code>, or processes launched from an activated shell</td></tr>
        <tr><td>Project and folder mappings</td><td>Approved <code>.synaps.yaml</code> files</td><td>You, Synaps, and the repository if you commit the file</td></tr>
        <tr><td>Scope approvals</td><td>Local SQLite database as a path and content digest</td><td>Synaps</td></tr>
      </tbody>
    </table>

    <h2 id="limits">What Synaps does not do</h2>
    <ul>
      <li>It does not sync memory or secrets to an account or hosted service.</li>
      <li>It does not send secret values through MCP, write them to YAML, or accept them as command arguments.</li>
      <li>It cannot directly modify an already-running parent process. Use <code>synaps run -- &lt;command&gt;</code> for one child, or explicitly evaluate <code>synaps hook</code> so your shell applies quoted environment changes itself.</li>
      <li>It does not decide that every conversation detail deserves permanent memory. Connected tools receive instructions to keep stable, confirmed context.</li>
      <li>The current beta is a signed Apple-silicon build for macOS 13 or later.</li>
    </ul>

    <h2 id="paths">Choose a path</h2>
    <dl>
      <dt>I want to use Synaps now.</dt>
      <dd>Follow <a href="../tutorials/connect/">Install and connect your first tools</a>. It covers download, first launch, the CLI, both supported tools, and a working recall check.</dd>
      <dt>I want to manage Synaps from the app.</dt>
      <dd>Use the <a href="app/">desktop app reference</a> for every screen, status, confirmation, editor, and shell-integration control.</dd>
      <dt>I want to understand memory behavior.</dt>
      <dd>Read <a href="memory/">Memory and recall</a>, then follow <a href="../tutorials/continuity/">Carry one decision between tools</a>.</dd>
      <dt>I need credentials for local commands.</dt>
      <dd>Read <a href="vault/">Vaults and scopes</a>, then build a complete approved scope in <a href="../tutorials/secrets/">Use a scoped secret in either shell mode</a>.</dd>
      <dt>I need automation or exact syntax.</dt>
      <dd>Use the <a href="cli/">complete CLI reference</a> and <a href="mcp/">MCP tool reference</a>.</dd>
      <dt>I am planning backups or a recovery.</dt>
      <dd>Read <a href="data/">Data lifecycle</a> before exporting or restoring the database.</dd>
    </dl>
  `,
};

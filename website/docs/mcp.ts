import { code, note } from "../markup";
import type { Page } from "../types";

export const mcp: Page = {
  path: "docs/mcp/index.html",
  title: "MCP tool reference",
  description: "Run the local stdio server and understand the exact request, response, and value boundary of every exposed tool.",
  kind: "docs",
  toc: [
    { label: "Server", id: "server" },
    { label: "remember", id: "remember" },
    { label: "recall", id: "recall" },
    { label: "vaultstatus", id: "vaultstatus" },
    { label: "Mesh tools", id: "mesh" },
    { label: "Operational behavior", id: "behavior" },
  ],
  body: `
    <h2 id="server">Server</h2>
    <p>Synapse implements an MCP stdio server. A tool launches the same signed or installed executable with the <code>mcp</code> argument:</p>
    ${code("shell", `~/.local/bin/synapse mcp`)}
    <p>The process uses stdin and stdout for protocol messages. Do not wrap it in a command that writes banners or shell setup output to stdout. The server opens the same local database as the desktop app and holds a shared lifecycle lock while connected.</p>

    <h2 id="remember">remember</h2>
    <p>Stores a durable fact, decision, preference, convention, or correction.</p>
    ${code("json", `{
  "content": "Use small focused modules.",
  "source": "synapse",
  "scope": "project",
  "project": "/Users/example/project"
}`)}
    <table>
      <thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td><code>content</code></td><td>string</td><td>Yes</td><td>The durable text. Empty or whitespace-only content is rejected.</td></tr>
        <tr><td><code>source</code></td><td>string or null</td><td>No</td><td>An origin such as a project path, repository name, or topic.</td></tr>
        <tr><td><code>scope</code></td><td><code>project</code> or <code>global</code></td><td>No</td><td>Defaults to project. Use global only for context that should appear everywhere.</td></tr>
        <tr><td><code>project</code></td><td>string or null</td><td>For project scope</td><td>Absolute working-project path. Synapse normalizes nested paths to the project root.</td></tr>
      </tbody>
    </table>
    ${code("json", `{
  "id": 24,
  "stored": true
}`)}

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

    <h2 id="mesh">Mesh tools</h2>
    <p>Three tools are always present. Sixteen more appear only while the agent mesh is switched on, because a tool definition costs context in every session that loads it. They let connected sessions register under a name, message each other directly or by channel, park on <code>wait</code> until work arrives, report and watch work state, and start or stop background workers.</p>
    <p>The guidance explaining them is sent with them and withdrawn with them, so the tool list and the instructions can never be out of step. See the <a href="../mesh/">agent mesh guide</a> for the full list and what each one is for.</p>

    <h2 id="behavior">Operational behavior</h2>
    <ul>
      <li>Tool errors are returned as readable strings. Protocol messages remain on stdio.</li>
      <li>The server identifies itself as <code>synapse</code> using the application version and advertises tool capability only.</li>
      <li>Opening the database runs integrity and relationship checks, applies numbered migrations, and secures database permissions.</li>
      <li>A running MCP process holds a shared database lock. <code>synapse data restore</code> requires an exclusive lock and therefore refuses while any connected server or the desktop app is using the database.</li>
      <li>Connected tools should recall before decisions that depend on project history and remember only stable confirmed context after it is established.</li>
    </ul>
  `,
};

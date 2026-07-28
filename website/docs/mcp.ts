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
  "source": "synapse"
}`)}
    <table>
      <thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td><code>content</code></td><td>string</td><td>Yes</td><td>The durable text. Empty or whitespace-only content is rejected.</td></tr>
        <tr><td><code>source</code></td><td>string or null</td><td>No</td><td>An origin such as a project path, repository name, or topic.</td></tr>
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
  "limit": 8
}`)}
    <table>
      <thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Meaning</th></tr></thead>
      <tbody>
        <tr><td><code>query</code></td><td>string</td><td>Yes</td><td>Words or a phrase describing the context needed. Use an empty string for recent entries.</td></tr>
        <tr><td><code>limit</code></td><td>integer or null</td><td>No</td><td>Requested result count. Defaults to 8. The active response budget may lower it.</td></tr>
      </tbody>
    </table>
    ${code("json", `{
  "memories": [
    {
      "id": 24,
      "body": "Use small focused modules.",
      "source": "synapse",
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

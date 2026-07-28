import { code } from "../markup";
import type { Page } from "../types";

export const config: Page = {
  path: "docs/config/index.html",
  title: "Configuration and paths",
  description: "Find every Synapse-owned path, understand environment overrides, and know how integration and editor writes are protected.",
  kind: "docs",
  toc: [
    { label: "Resolved paths", id: "paths" },
    { label: "Environment variables", id: "environment" },
    { label: "Tool files", id: "tools" },
    { label: "Safe writes", id: "writes" },
    { label: "Settings and shell", id: "settings" },
  ],
  body: `
    <h2 id="paths">Resolved paths</h2>
    ${code("shell", `synapse path`)}
    <table>
      <thead><tr><th>Item</th><th>Default on macOS</th></tr></thead>
      <tbody>
        <tr><td>Home</td><td><code>~</code></td></tr>
        <tr><td>Data directory</td><td><code>~/Library/Application Support/synapse</code></td></tr>
        <tr><td>Database</td><td><code>~/Library/Application Support/synapse/brain.db</code></td></tr>
        <tr><td>Installed CLI</td><td><code>~/.local/bin/synapse</code></td></tr>
      </tbody>
    </table>

    <h2 id="environment">Environment variables</h2>
    <table>
      <thead><tr><th>Name</th><th>Effect</th><th>Typical use</th></tr></thead>
      <tbody>
        <tr><td><code>SYNAPSE_DATA</code></td><td>Replaces the application-data directory.</td><td>Development, tests, or a deliberately isolated database.</td></tr>
        <tr><td><code>SYNAPSE_HOME</code></td><td>Replaces the home directory used for tool files and the default CLI path.</td><td>Integration testing without touching real user files.</td></tr>
        <tr><td><code>SYNAPSE_BIN</code></td><td>Replaces the full CLI installation destination.</td><td>Install outside <code>~/.local/bin</code>.</td></tr>
        <tr><td><code>SYNAPSE_DOCUMENT</code></td><td>Launches the desktop app directly into a Markdown, TOML, JSON, or YAML document.</td><td>Internal editor entrypoint.</td></tr>
        <tr><td><code>SYNAPSE_PROJECT_DIR</code></td><td>Provides the fallback project folder for MCP <code>vaultstatus</code>.</td><td>A client that launches the server outside the project working directory.</td></tr>
        <tr><td><code>SYNAPSE_SHELL_ACTIVE</code></td><td>Identifies the shell hook currently evaluated in this shell.</td><td>Set automatically by the zsh, bash, or fish integration so status can report it.</td></tr>
      </tbody>
    </table>
    ${code("shell", `SYNAPSE_DATA="$HOME/tmp/synapsedemo" synapse data check
SYNAPSE_BIN="$HOME/bin/synapse" synapse install`)}
    <p>Overrides affect only the process that receives them. Keep the desktop app, CLI, and connected MCP server on the same <code>SYNAPSE_DATA</code> value if they should share one store.</p>

    <h2 id="tools">Tool integration files</h2>
    <table>
      <thead><tr><th>Tool</th><th>Integration</th><th>Instructions</th><th>Settings shortcut</th></tr></thead>
      <tbody>
        <tr><td>Codex</td><td><code>~/.codex/config.toml</code></td><td><code>~/.codex/AGENTS.md</code></td><td><code>~/.codex/config.toml</code></td></tr>
        <tr><td>Claude Code</td><td><code>~/.claude.json</code></td><td><code>~/.claude/CLAUDE.md</code></td><td><code>~/.claude/settings.json</code></td></tr>
      </tbody>
    </table>
    <p>Connection detection parses the actual TOML or JSON entry named <code>synapse</code>. It reports connected only when the stored command resolves to the expected executable and the arguments equal <code>["mcp"]</code>. A deleted development binary therefore appears stale instead of healthy.</p>

    <h2 id="writes">Safe writes</h2>
    <p>The built-in document views validate JSON, TOML, and YAML before saving. A write with changed content:</p>
    <ol>
      <li>Resolves a dotfile symlink and writes through it rather than replacing the link.</li>
      <li>Writes the previous bytes to a sibling file ending in <code>.synapsebackup</code>.</li>
      <li>Writes the replacement to a temporary file, syncs it, preserves existing permissions, then renames it atomically.</li>
      <li>Syncs the containing directory.</li>
    </ol>
    <p>Tool setup snapshots both its integration and instruction files. If registration or the instruction update fails, it restores both prior states. The CLI installer applies the same rollback model to its launcher and receipt.</p>

    <h2 id="settings">Settings and shell integration</h2>
    ${code("shell", `synapse settings show
synapse settings optimize full
synapse settings optimize balanced
synapse settings optimize lean

# Manual alternative to Settings → Enable shell hook.
eval "$(synapse hook zsh)"
synapse allow
synapse deny`)}
    <p>The optimization value lives in SQLite, so the app, CLI, and every connected MCP process read the same setting. Balanced is the default. See <a href="../memory/">Memory and recall</a> for exact limits and transformations.</p>
    <p>The Settings screen detects the default shell and can enable, repair, or remove its managed startup-file block. Existing files are backed up and replaced atomically. The hook is an explicit per-shell opt-in: it activates only inside a directory with at least one discovered, approved scope. Leaving the scope, revoking approval, or changing the YAML unloads managed values and restores values that existed before activation. See <a href="../vault/#modes">Choose an environment boundary</a> before enabling ambient mode.</p>
  `,
};

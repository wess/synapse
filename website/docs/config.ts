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
        <tr><td>Shared guidance</td><td><code>~/Library/Application Support/synapse/SOUL.md</code></td></tr>
        <tr><td>Skill library</td><td><code>~/Library/Application Support/synapse/skills/</code></td></tr>
        <tr><td>Mesh roles and teams</td><td><code>~/Library/Application Support/synapse/roles/</code>, <code>teams/</code></td></tr>
        <tr><td>Worker logs</td><td><code>~/Library/Application Support/synapse/workers/</code></td></tr>
        <tr><td>Database backups</td><td><code>~/Library/Application Support/synapse/backups/</code></td></tr>
        <tr><td>Installed CLI</td><td><code>~/.local/bin/synapse</code></td></tr>
      </tbody>
    </table>
    <p>Everything above lives under one folder. A project may also hold <code>.synapse.yaml</code> scope files and a <code>.synapse/roles/</code> directory, both of which travel with the checkout rather than with you.</p>

    <h2 id="environment">Environment variables</h2>
    <table>
      <thead><tr><th>Name</th><th>Effect</th><th>Typical use</th></tr></thead>
      <tbody>
        <tr><td><code>SYNAPSE_DATA</code></td><td>Replaces the application-data directory.</td><td>Development, tests, or a deliberately isolated database.</td></tr>
        <tr><td><code>SYNAPSE_HOME</code></td><td>Replaces the home directory used for tool files and the default CLI path.</td><td>Integration testing without touching real user files.</td></tr>
        <tr><td><code>SYNAPSE_BIN</code></td><td>Replaces the full CLI installation destination.</td><td>Install outside <code>~/.local/bin</code>.</td></tr>
        <tr><td><code>SYNAPSE_PROJECT_DIR</code></td><td>Provides the fallback project folder for MCP calls that do not carry one, and is what a launched agent is told to treat as home.</td><td>A client that launches the server outside the project working directory. Set automatically by <code>synapse launch</code> and <code>relay launch</code>.</td></tr>
        <tr><td><code>SYNAPSE_PAGE</code></td><td>Chooses which dashboard page opens first: <code>memory</code>, <code>mesh</code>, <code>skills</code>, <code>vaults</code>, or <code>settings</code>. Anything else opens Connections.</td><td>Opening the app straight to the screen you want.</td></tr>
        <tr><td><code>CODEX_HOME</code></td><td>Replaces the Codex home directory used for detection and for reading an existing Codex memory store.</td><td>A non-default Codex installation, or testing an import without touching real files.</td></tr>
        <tr><td><code>SYNAPSE_SHELL_ACTIVE</code></td><td>Identifies the shell hook currently evaluated in this shell.</td><td>Set automatically by the zsh, bash, or fish integration so status can report it.</td></tr>
        <tr><td><code>SYNAPSE_SHELL_KEYS</code></td><td>Records which managed variables the hook currently has loaded, so leaving a scope unloads exactly those and restores what was there before.</td><td>Set automatically by the shell hook. Not intended to be set by hand.</td></tr>
        <tr><td><code>SYNAPSE_SHELL_COMMAND</code></td><td>The executable the installed shell block re-invokes on each directory change.</td><td>Set automatically by the startup-file block so a moved app is still reachable.</td></tr>
        <tr><td><code>SYNAPSE_DOCUMENT</code></td><td>Launches the desktop app directly into a Markdown, TOML, JSON, or YAML document.</td><td>Internal editor entrypoint.</td></tr>
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
    <p>The two instruction files contain managed pointers to the central <code>SOUL.md</code>. A normal sync preserves other text in place. The optional consolidation action first moves that text into <code>SOUL.md</code>, then leaves both global files pointer-only so shared guidance has one editable source.</p>
    <p>Connecting Claude Code also writes two entries into <code>~/.claude/settings.json</code>: a <code>SessionStart</code> hook running <code>synapse session</code>, and a <code>statusLine</code> running <code>synapse statusline</code>. JSON has no comment syntax, so Synapse cannot mark its own entries the way it marks a block in a Markdown file. It recognizes them by the command they run instead, and carries everything else through untouched — a status line you configured yourself is reported and left alone, never replaced.</p>
    <table>
      <thead><tr><th>Entry</th><th>Command</th><th>What it produces</th></tr></thead>
      <tbody>
        <tr><td><code>hooks.SessionStart</code></td><td><code>synapse session</code></td><td>The connection line in your terminal, and this project's memory in the session's context before the first turn. See <a href="../mcp/#sessionstart">Session start</a>.</td></tr>
        <tr><td><code>statusLine</code></td><td><code>synapse statusline</code></td><td>One line under the prompt for the rest of the session: model, folder, memory count, and mesh size.</td></tr>
      </tbody>
    </table>
    <p>Both are removed by <code>synapse disconnect claude</code>. Codex exposes neither, so a Codex connection is the MCP entry and the instruction pointer only.</p>

    <h2 id="writes">Safe writes</h2>
    <p>The built-in document views validate JSON, TOML, and YAML before saving. A write with changed content:</p>
    <ol>
      <li>Resolves a dotfile symlink and writes through it rather than replacing the link.</li>
      <li>Writes the previous bytes to a sibling file ending in <code>.synapsebackup</code>.</li>
      <li>Writes the replacement to a temporary file, syncs it, preserves existing permissions, then renames it atomically.</li>
      <li>Syncs the containing directory.</li>
    </ol>
    <p>Tool setup snapshots its integration file, instruction file, and shared guidance file. If registration or the instruction update fails, it restores every prior state. The CLI installer applies the same rollback model to its launcher and receipt.</p>

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

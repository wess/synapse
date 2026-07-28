import { code, note } from "../markup";
import type { Page } from "../types";
import { releaseurl } from "../deploy";

export const install: Page = {
  path: "docs/install/index.html",
  title: "Install and connect",
  description: "Install the signed macOS beta, add the CLI, and connect Codex or Claude Code without replacing your configuration.",
  kind: "docs",
  toc: [
    { label: "Requirements", id: "requirements" },
    { label: "Install the app", id: "app" },
    { label: "Install the CLI", id: "cli" },
    { label: "Shell integration", id: "shell" },
    { label: "Connect tools", id: "tools" },
    { label: "Manual connection", id: "manual" },
    { label: "Verify", id: "verify" },
  ],
  body: `
    <h2 id="requirements">Requirements</h2>
    <ul>
      <li>Apple-silicon Mac running macOS 13 or later.</li>
      <li>At least one supported tool on <code>PATH</code>: Codex or Claude Code.</li>
      <li>A writable <code>~/.local/bin</code>, or a custom path supplied through <code>SYNAPSE_BIN</code>.</li>
    </ul>

    <h2 id="app">Install the app</h2>
    <ol>
      <li>Download <code>synapse.zip</code> from the <a href="${releaseurl}">current beta release</a>.</li>
      <li>Extract the archive and move <strong>synapse.app</strong> into <strong>Applications</strong>. Do not install the CLI while the app is still inside Downloads or a mounted disk image.</li>
      <li>Open Synapse. The app is Developer ID signed and notarized by Apple, so it opens under default Gatekeeper settings with no security override.</li>
    </ol>
    ${note("Why move it first?", "The installed CLI is a small launcher that points into the signed application bundle. Moving the app later would leave that launcher pointing at the old location.")}

    <h2 id="cli">Install the CLI</h2>
    <p>Open <strong>Settings</strong> in Synapse and choose <strong>Install CLI</strong>, or run the application binary once from a terminal and use:</p>
    ${code("shell", `synapse install
synapse path
synapse version`)}
    <p>The default destination is <code>~/.local/bin/synapse</code>. If your shell cannot find it, add that folder to <code>PATH</code> and start a new shell.</p>
    ${code("shell", `export PATH="$HOME/.local/bin:$PATH"`)}
    <p>Synapse never overwrites an unrelated executable. A managed launcher has a sibling <code>.synapsereceipt</code> containing its digest so future installs can distinguish an update from a conflict.</p>

    <h2 id="shell">Enable shell integration</h2>
    <p>In <strong>Settings → Shell environments</strong>, choose <strong>Enable shell hook</strong>. Synapse detects your default zsh, bash, or fish shell, installs the CLI if needed, and adds one marked block to its startup file. Open a new terminal after the setting changes.</p>
    <table>
      <thead><tr><th>Shell</th><th>Managed startup file</th></tr></thead>
      <tbody>
        <tr><td>zsh</td><td><code>~/.zshrc</code></td></tr>
        <tr><td>bash on macOS</td><td><code>~/.bash_profile</code></td></tr>
        <tr><td>fish</td><td><code>~/.config/fish/config.fish</code></td></tr>
      </tbody>
    </table>
    <p>The app shows <strong>Needs repair</strong> if the managed block changes. Repair replaces only that block. Remove deletes only that block; already-running terminals retain the loaded hook until they close. Every changed existing startup file receives a <code>.synapsebackup</code> sibling and an atomic replacement.</p>

    <h2 id="tools">Connect tools</h2>
    <p>On the Synapse dashboard, each detected tool shows its installation and connection state. Choose <strong>Connect</strong> for Codex or Claude Code.</p>
    <p>Setup performs two changes as one rollback-protected operation:</p>
    <ol>
      <li>It registers the installed Synapse executable as a user-level MCP stdio server with the single argument <code>mcp</code>.</li>
      <li>It appends or refreshes a delimited Synapse memory block in the tool’s global instruction file. Existing user content remains outside that block.</li>
    </ol>
    <p>The connected tool reads that global block on launch, and the MCP server repeats the same policy during initialization. It explicitly tells the tool to recall relevant context at the start of every session, remember confirmed reusable facts proactively, use Synapse instead of ad hoc memory Markdown files, and keep secrets out of memory.</p>
    <table>
      <thead><tr><th>Tool</th><th>MCP store</th><th>Instruction file</th></tr></thead>
      <tbody>
        <tr><td>Codex</td><td><code>~/.codex/config.toml</code></td><td><code>~/.codex/AGENTS.md</code></td></tr>
        <tr><td>Claude Code</td><td><code>~/.claude.json</code></td><td><code>~/.claude/CLAUDE.md</code></td></tr>
      </tbody>
    </table>
    <p>Before changing a tool store or instruction file, Synapse creates a sibling <code>.synapsebackup</code>. If either half of setup fails, both files are restored.</p>

    <h2 id="manual">Manual connection</h2>
    <p>The app is the recommended setup path because it detects stale executable paths and repairs them. If you need to register the server manually, use the installed CLI path:</p>
    ${code("shell", `codex mcp add synapse -- ~/.local/bin/synapse mcp
claude mcp add --scope user synapse -- ~/.local/bin/synapse mcp`)}
    <p>Then add the memory instructions shown in the app to the corresponding global instruction file. Do not point an integration at a build artifact such as <code>target/debug/synapse</code>; cleaning the repository would break the connection.</p>

    <h2 id="verify">Verify the connection</h2>
    <ol>
      <li>Restart the connected tool so it reloads its MCP servers and instructions.</li>
      <li>Inspect its available tools. Synapse should expose <code>remember</code>, <code>recall</code>, and <code>vaultstatus</code>.</li>
      <li>Ask it to remember one harmless confirmed convention, then recall it in a new session.</li>
      <li>Open the Synapse <strong>Memories</strong> screen and confirm the exact entry and source are visible.</li>
    </ol>
    <p>If the dashboard shows a registered but disconnected state, the stored path is stale or its arguments differ from <code>["mcp"]</code>. Choose <strong>Repair</strong> in the app or remove and re-add the manual entry.</p>
  `,
};

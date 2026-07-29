import { code, note } from "../markup";
import type { Page } from "../types";

export const connect: Page = {
  path: "tutorials/connect/index.html",
  title: "Install and connect your first tools",
  description: "Go from the signed beta archive to a working local MCP connection and verify memory from the app, CLI, and developer tool.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Install", id: "install" },
    { label: "Connect", id: "connect" },
    { label: "Verify", id: "verify" },
    { label: "Next", id: "next" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will install Synapse in Applications, install its user CLI, connect at least one supported developer tool, store a harmless test memory, and verify that the same record is visible through three surfaces.</p>
    <ul>
      <li>Apple-silicon Mac running macOS 13 or later.</li>
      <li>Codex, Claude Code, or both installed and available on <code>PATH</code>.</li>
      <li>The latest <code>synapse.zip</code> release archive.</li>
    </ul>

    <ol class="steps">
      <li>
        <h3 id="install">Move and open the app</h3>
        <p>Extract <code>synapse.zip</code>, move <strong>synapse.app</strong> to Applications, then open it. The app is Developer ID signed and notarized, so it opens without a security override.</p>
        ${note("Choose the permanent location now", "The CLI installed from a packaged app launches the executable inside that signed bundle. Install it only after the app is where you intend to keep it.")}
      </li>
      <li>
        <h3>Install the CLI</h3>
        <p>Open Synapse Settings and choose <strong>Install CLI</strong>. Then open a new terminal:</p>
        ${code("shell", `synapse version
synapse path`)}
        <p>If the shell cannot find Synapse, add <code>~/.local/bin</code> to <code>PATH</code> and start another shell. The path output should point at the same data directory the app uses.</p>
      </li>
      <li>
        <h3 id="connect">Connect a detected tool</h3>
        <p>Return to the dashboard. Choose <strong>Connect</strong> beside Codex or Claude Code. The row should move from detected to connected. Connect the second tool too if it is installed.</p>
        <p>Synapse registers <code>~/.local/bin/synapse mcp</code> at user scope, creates the shared <code>SOUL.md</code>, and adds a managed pointer to it. Existing configuration and instruction text remain in place; changed existing files receive <code>.synapsebackup</code> siblings.</p>
      </li>
      <li>
        <h3>Restart the developer tool</h3>
        <p>Close and reopen the connected tool so it loads the new MCP server. Its first reply of the session should open with a line such as <code>Synapse connected · 3 memories recalled</code>, which is how a connected tool reports the live link. Inspect its available tools or ask it what Synapse tools are available. The answer should include <code>remember</code>, <code>recall</code>, and <code>vaultstatus</code>.</p>
      </li>
      <li>
        <h3 id="verify">Store a test memory</h3>
        <p>Tell the connected tool:</p>
        <blockquote>Remember this confirmed tutorial convention: documentation examples use the source label synapsetutorial.</blockquote>
        <p>It should call <code>remember</code> with the current project root and report a stored numeric ID. Do not use a secret or sensitive value for this check.</p>
      </li>
      <li>
        <h3>Verify through the CLI and app</h3>
        ${code("shell", `synapse memory list synapsetutorial
synapse memory list synapsetutorial --json`)}
        <p>Open the Synapse Memories screen and search for <code>synapsetutorial</code>. Confirm the exact body and source match. At this point the developer tool, CLI, and desktop app are reading one local store.</p>
      </li>
    </ol>

    <h2 id="next">Next step</h2>
    <p>Continue with <a href="../continuity/">Carry one decision between tools</a> to verify a real cross-session handoff. If a tool row becomes disconnected after moving or rebuilding Synapse, choose <strong>Repair</strong>; the app recognizes stale executable paths.</p>
  `,
};

import { code, note } from "../markup";
import type { Page } from "../types";

export const connect: Page = {
  path: "tutorials/connect/index.html",
  title: "Install and connect your first tools",
  description:
    "Go from the signed release archive to a working local MCP connection, and verify the same memory from the desktop app, the terminal, and the coding tool itself.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Install the app", id: "install" },
    { label: "Install the CLI", id: "cli" },
    { label: "Connect a tool", id: "connect" },
    { label: "What connecting wrote", id: "wrote" },
    { label: "Restart the tool", id: "restart" },
    { label: "Store a memory", id: "store" },
    { label: "Verify three ways", id: "verify" },
    { label: "If something is wrong", id: "wrong" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will install Synapse, install its command-line tool, connect at least one coding tool, store one harmless memory, and see that same record through three separate surfaces. By the end you will also know exactly which files were touched and how to take every one of them back.</p>
    <ul>
      <li>Apple-silicon Mac running macOS 13 or later.</li>
      <li>Codex, Claude Code, or both installed and on <code>PATH</code>.</li>
      <li>The latest <code>synapse.zip</code> release archive.</li>
      <li>About twenty minutes.</li>
    </ul>

    <ol class="steps">
      <li>
        <h3 id="install">Move the app somewhere permanent, then open it</h3>
        <p>Extract <code>synapse.zip</code>, move <strong>synapse.app</strong> to Applications, and open it. The app is Developer ID signed and notarized, so it opens without a security override and without a right-click workaround.</p>
        ${note("Choose the location before the next step", "The CLI installed from a packaged app launches the executable inside that signed bundle. Install the CLI only after the app is where you intend to keep it, or the command will point at a path that no longer exists. If you move the app later, the dashboard offers <strong>Repair</strong> — it recognizes a stale executable path rather than reporting a healthy connection.")}
      </li>

      <li>
        <h3 id="cli">Install the CLI</h3>
        <p>Open Synapse Settings and choose <strong>Install CLI</strong>, or run the installer from inside the bundle. Either way it reports where it landed:</p>
        ${code("text", `Installed /Users/example/.local/bin/synapse
Add /Users/example/.local/bin to PATH to use synapse from your shell.`)}
        <p>Open a new terminal and confirm the shell can find it:</p>
        ${code("shell", `synapse version
synapse path`)}
        ${code("text", `synapse 0.1.0-beta.24`)}
        ${code("text", `data	/Users/example/Library/Application Support/synapse
soul	/Users/example/Library/Application Support/synapse/SOUL.md
cli	/Users/example/.local/bin/synapse`)}
        <p>If the shell cannot find <code>synapse</code>, add <code>~/.local/bin</code> to <code>PATH</code> and start another shell. The <code>data</code> path above must match what the app reports on its Settings screen — if the two differ, something has set <code>SYNAPSE_DATA</code> in one context and not the other, and they are reading different stores.</p>
      </li>

      <li>
        <h3 id="connect">Connect a detected tool</h3>
        <p>Return to the dashboard. Each supported tool appears as a row with its state. Choose <strong>Connect</strong> beside Codex, Claude Code, or pi, and the row moves from detected to connected. Connect the others too if you have them — the whole point is one memory behind all of them.</p>
        <p>Detection is not a guess. Synapse parses the actual entry named <code>synapse</code> in the tool's own configuration and reports connected only when the stored command resolves to the expected executable and its arguments are exactly <code>["mcp"]</code>. A tool whose binary was deleted or moved reads as stale, not healthy.</p>
      </li>

      <li>
        <h3 id="wrote">Know what connecting actually wrote</h3>
        <p>This is worth understanding now rather than the first time something looks wrong. Connecting is not a black box — it is four specific changes, and every one is reversible.</p>
        <table>
          <thead><tr><th>Change</th><th>Where</th><th>How it is made</th></tr></thead>
          <tbody>
            <tr><td>The MCP server</td><td><code>~/.codex/config.toml</code> or <code>~/.claude.json</code></td><td>Through the tool's own <code>mcp add</code> command, not by editing its file directly.</td></tr>
            <tr><td>A guidance pointer</td><td><code>~/.codex/AGENTS.md</code> or <code>~/.claude/CLAUDE.md</code></td><td>Appended inside a managed block. Your own text is untouched.</td></tr>
            <tr><td>A session hook</td><td><code>~/.claude/settings.json</code></td><td>Claude Code only. Runs <code>synapse session</code> at session start.</td></tr>
            <tr><td>A status line</td><td><code>~/.claude/settings.json</code></td><td>Claude Code only, and only if you do not already have one. Yours is reported, never replaced.</td></tr>
          </tbody>
        </table>
        <p>Every changed file gets a <code>.synapsebackup</code> sibling before it is replaced, and the whole operation rolls back if any step fails. Check the shared guidance file was created:</p>
        ${code("shell", `synapse guidance show`)}
        ${code("text", `SOUL.md	/Users/example/Library/Application Support/synapse/SOUL.md
exists	true
pointers	2/2
consolidated	false`)}
        <p>A full count — <code>pointers 3/3</code> with all three tools connected — means each one points at a single editable <code>SOUL.md</code>. That file is yours to edit; it is where shared guidance for every connected tool lives.</p>
      </li>

      <li>
        <h3 id="restart">Restart the tool and read its first line</h3>
        <p>Close and reopen the connected tool so it loads the new MCP server. A running session keeps the tool list it started with, so this step is not optional.</p>
        <p>Claude Code will print a line beside its welcome box before the model has written anything:</p>
        ${code("text", `Synapse connected · no memories yet`)}
        <p>That line comes from the session hook, which is the only way to state the connection before the first reply. It also hands the session this project's memory directly, so a Claude Code session starts already holding what the project has decided rather than being asked to go and look. On an empty store there is nothing to hand over yet, which is what you are seeing.</p>
        <p>Codex has no session hook, so it reports the connection in its first reply instead, following the guidance in <code>SOUL.md</code>. Either way, ask the tool what Synapse tools it has. The answer should include <code>remember</code>, <code>recall</code>, and <code>vaultstatus</code> — three, not more. The sixteen mesh tools appear only when you switch the mesh on.</p>
      </li>

      <li>
        <h3 id="store">Store one memory</h3>
        <p>Tell the connected tool:</p>
        <blockquote>Remember this confirmed tutorial convention: documentation examples use the source label synapsetutorial.</blockquote>
        <p>It should call <code>remember</code> with the current project root and report a numeric ID. Use an ordinary convention for this — memory is plain text readable by every connected tool, so never put a token, password, or private key in it.</p>
      </li>

      <li>
        <h3 id="verify">Verify the same record three ways</h3>
        <p>One store, three surfaces. Check all of them, because a mismatch here is the clearest signal that something is pointed at the wrong place.</p>
        ${code("shell", `synapse memory list synapsetutorial`)}
        ${code("text", `1	project:/Users/example/project	synapsetutorial	Documentation examples use the source label synapsetutorial.`)}
        <p>The columns are the ID, the scope and project it belongs to, the source label, and the body. Now the exact record:</p>
        ${code("shell", `synapse memory show 1`)}
        ${code("text", `Memory #1
Scope: project
Project: /Users/example/project
Source: synapsetutorial
Created: 1785776940

Documentation examples use the source label synapsetutorial.`)}
        <p>Finally, open the <strong>Memories</strong> screen in the app and search for <code>synapsetutorial</code>. The body, source, and scope must match what the terminal just printed. At this point the coding tool, the CLI, and the desktop app are provably reading one local database.</p>
      </li>
    </ol>

    <h2 id="wrong">If something is not right</h2>
    <table>
      <thead><tr><th>Symptom</th><th>What it usually means</th></tr></thead>
      <tbody>
        <tr><td>The tool lists no Synapse tools</td><td>It was not restarted after connecting. Close it fully and reopen.</td></tr>
        <tr><td>The dashboard row says stale</td><td>The app moved after the connection was made. Choose <strong>Repair</strong>.</td></tr>
        <tr><td><code>synapse</code>: command not found</td><td><code>~/.local/bin</code> is not on <code>PATH</code>, or the shell has not been restarted.</td></tr>
        <tr><td>CLI and app show different memory counts</td><td>They are on different data directories. Compare <code>synapse path</code> with the app's Settings screen.</td></tr>
        <tr><td>The tool reports a connection that is not there</td><td>Run <code>synapse doctor</code>. It reports what is actually configured rather than what should be.</td></tr>
      </tbody>
    </table>
    ${code("shell", `synapse doctor`)}
    <p>That one command is also what to attach to a bug report. It carries no memory contents and no secret names, so it is safe to paste into a public issue.</p>

    <h2>Undoing all of it</h2>
    ${code("shell", `synapse disconnect claude     # or codex, or nothing for every tool`)}
    <p>Disconnecting removes only what Synapse wrote: the MCP entry, the managed block in the instruction file, the session hook, and the status line — and only if that status line is the one Synapse installed. Your own words in <code>CLAUDE.md</code>, your own status line, and every memory survive it. Removing memory is a separate, explicit decision covered in <a href="../lifecycle/">Check, migrate, and remove Synapse</a>.</p>

    <h2>Next step</h2>
    <p>Continue with <a href="../continuity/">Carry one decision between tools</a> to prove a real handoff across a session boundary and a tool boundary.</p>
  `,
};

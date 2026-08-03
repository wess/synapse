import { code, note } from "../markup";
import type { Page } from "../types";

export const launch: Page = {
  path: "tutorials/launch/index.html",
  title: "Start a tool with everything in place",
  description:
    "Use synapse launch to open Codex or Claude Code with memory, scoped credentials, and the project root already wired, without writing anything into that tool's own configuration.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Preview first", id: "preview" },
    { label: "Read the preview", id: "read" },
    { label: "Add credentials", id: "credentials" },
    { label: "Watch it refuse", id: "refuse" },
    { label: "Pass flags through", id: "flags" },
    { label: "Launch for real", id: "launch" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will open a coding tool that has memory, the folder's scoped credentials, and the right project root from its first turn — and confirm that your machine is exactly as it was afterwards. <code>synapse launch</code> wires a tool for the life of one process. It is not a setup step and it writes nothing into the tool's own configuration.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and on <code>PATH</code>.</li>
      <li>Codex or Claude Code installed. Neither needs to be connected — that is the point of this tutorial.</li>
      <li>A scratch project folder. Steps four and five create a disposable Keychain value.</li>
    </ul>

    ${note("Launching is not connecting", "<code>synapse connect</code> is a decision you make once: it registers the MCP server in the tool's own configuration and leaves it there. <code>synapse launch</code> makes no such change. If the tool has no connection of its own, Synapse hands it a generated configuration for that one process and the machine is unchanged when it exits.")}

    <ol class="steps">
      <li>
        <h3 id="preview">Preview before you launch</h3>
        <p>Work in a scratch folder so nothing here touches a real project:</p>
        ${code("shell", `mkdir -p "$HOME/tmp/launchtutorial" && cd "$HOME/tmp/launchtutorial"
git init --quiet .
synapse launch claude --print`)}
        <p><code>--print</code> resolves everything and shows you the result instead of running it:</p>
        ${code("text", `/Users/example/.local/bin/claude --mcp-config ~/Library/Application Support/synapse/relay/launch.66c4a25fcfe8.mcp.json
env  SYNAPSE_PROJECT_DIR=/Users/example/tmp/launchtutorial`)}
        <p>Try Codex too. It reads no MCP configuration file, so it gets the server on its command line instead:</p>
        ${code("shell", `synapse launch codex --print`)}
        ${code("text", `/Users/example/.asdf/shims/codex -c mcp_servers.synapse.command="/Users/example/.local/bin/synapse" -c mcp_servers.synapse.args=["mcp"]
env  SYNAPSE_PROJECT_DIR=/Users/example/tmp/launchtutorial`)}
      </li>

      <li>
        <h3 id="read">Read what it actually did</h3>
        <p>Three things were resolved, and each is worth understanding before you trust the command with a real project.</p>
        <table>
          <thead><tr><th>Piece</th><th>Where it came from</th></tr></thead>
          <tbody>
            <tr><td>The program</td><td>Looked up in the registry of connectable tools, then on <code>PATH</code>. A tool that is not installed fails here with a name, not a shell error.</td></tr>
            <tr><td>The MCP wiring</td><td>Written <em>only</em> because this tool has no Synapse connection of its own. A connected tool is launched as-is and gets no generated file at all.</td></tr>
            <tr><td><code>SYNAPSE_PROJECT_DIR</code></td><td>The folder you launched from, walked up to its project root. This is what scopes the tool's memory and, on the mesh, its registration.</td></tr>
          </tbody>
        </table>
        <p>The generated configuration is keyed on a digest of the project root, so relaunching in the same folder reuses one file rather than accumulating them. Look at it if you like — it names this binary and the <code>mcp</code> argument, and nothing else.</p>
        ${note("Why this exists at all", "Before <code>launch</code>, wiring a tool meant either connecting it permanently or assembling the arguments by hand. The first is a commitment you might not want on a machine you are borrowing; the second is easy to get subtly wrong. <code>launch</code> works before setup has ever run and leaves nothing behind.")}
      </li>

      <li>
        <h3 id="credentials">Add scoped credentials</h3>
        <p>A launched tool can run a shell, so it gets the same scoped environment <code>synapse run</code> would give a child. Create a disposable value and map it to this folder:</p>
        ${code("shell", `synapse vault create demo
synapse secret set demo token DEMO_TOKEN`)}
        <p>Enter a throwaway value at the hidden prompt. Then create the scope and approve it:</p>
        ${code("shell", `synapse scope init .`)}
        ${code("yaml", `version: 1
scope: project
env:
  DEMO_TOKEN: demo.token
deny: []`)}
        ${code("shell", `synapse allow
synapse launch claude --print`)}
        <p>The preview now names the variable and refuses to show what is in it:</p>
        ${code("text", `/Users/example/.local/bin/claude --mcp-config …/launch.66c4a25fcfe8.mcp.json
env  SYNAPSE_PROJECT_DIR=/Users/example/tmp/launchtutorial
env  DEMO_TOKEN=<from keychain>`)}
        <p><code>&lt;from keychain&gt;</code> is not a redaction applied to a value that was read. A preview calls a different code path that lists names and never opens the Keychain at all, so there is no value in the process to leak.</p>
      </li>

      <li>
        <h3 id="refuse">Watch it refuse a half-resolved environment</h3>
        <p>Break the scope's approval by editing the file — a blank line is enough, because approval is bound to the exact bytes:</p>
        ${code("shell", `echo "" >> .synapse.yaml
synapse status .`)}
        ${code("text", `Folder: .
Available: none
Ambient: blocked
/Users/example/tmp/launchtutorial/.synapse.yaml [project · pending]
warning: /Users/example/tmp/launchtutorial/.synapse.yaml: Scope has not been approved`)}
        <p>Now try to launch:</p>
        ${code("shell", `synapse launch claude --print`)}
        ${code("text", `Error: vault scope is not ready:
/Users/example/tmp/launchtutorial/.synapse.yaml: Scope has not been approved`)}
        <p>It refuses rather than starting the tool with <code>DEMO_TOKEN</code> missing. That is deliberate and it is the same rule <code>synapse run</code> applies: a tool that can run a shell is never handed a partly-resolved environment, because the failure mode is a command that runs against the wrong thing and looks like it worked.</p>
        <p>Read the changed file, then approve it again:</p>
        ${code("shell", `synapse scope status .
synapse allow
synapse launch claude --print`)}
      </li>

      <li>
        <h3 id="flags">Pass the tool its own flags</h3>
        <p>Everything after a bare <code>--</code> reaches the tool untouched:</p>
        ${code("shell", `synapse launch claude --print -- --resume --model opus`)}
        ${code("text", `/Users/example/.local/bin/claude --mcp-config …/launch.66c4a25fcfe8.mcp.json --resume --model opus
env  SYNAPSE_PROJECT_DIR=/Users/example/tmp/launchtutorial
env  DEMO_TOKEN=<from keychain>`)}
        <p>The split happens before Synapse parses anything, which is why a flag both programs understand still reaches the right one. Without that, a <code>--model</code> meant for the tool would be eaten by Synapse.</p>
      </li>

      <li>
        <h3 id="launch">Launch for real</h3>
        ${code("shell", `synapse launch claude`)}
        <p>The tool opens as it normally would. Ask it what Synapse tools it has; the answer should include <code>remember</code>, <code>recall</code>, and <code>vaultstatus</code>. Ask it to call <code>vaultstatus</code> and it will report <code>DEMO_TOKEN</code> as available — the name only, because that tool returns metadata and cannot read the value.</p>
        <p>If this is Claude Code and it is also connected, its first message will carry this project's memory; see <a href="../../docs/mcp/#sessionstart">Session start</a>. A launched tool that is <em>not</em> connected has the MCP tools but not the session hook, because the hook lives in that tool's own settings and launching writes nothing there.</p>
        <p>Exit the tool. Then confirm the machine is unchanged:</p>
        ${code("shell", `synapse status`)}
        <p>The tool's own configuration was never opened. Nothing needs undoing.</p>
      </li>
    </ol>

    <h2 id="cleanup">Clean up</h2>
    ${code("shell", `synapse deny
synapse secret forget demo.token
synapse vault delete demo
cd .. && rm -rf launchtutorial`)}
    <p>Forgetting the secret removes both the Keychain item and Synapse's record of it. The generated MCP configuration under the data folder is harmless — it names this binary and nothing else — and is overwritten on the next launch in the same folder.</p>

    <h2>What you can rely on</h2>
    <ul>
      <li>Launching never edits the tool's own configuration. Making a connection permanent is <code>synapse connect</code>, a separate decision.</li>
      <li>A preview prints variable names and never a value, because it never reads one.</li>
      <li>Any scope warning refuses the launch outright rather than starting the tool with part of its environment.</li>
      <li>A connected tool is launched as-is. The generated configuration exists only for a tool that has no connection of its own.</li>
    </ul>

    <h2>Next step</h2>
    <p>Continue to <a href="../mesh/">Run a team of agents and drive it yourself</a>, which uses the same launch pipeline to open several tools at once and put you on the roster with them.</p>
  `,
};

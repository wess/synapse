import { code, note } from "../markup";
import type { Page } from "../types";

export const secrets: Page = {
  path: "tutorials/secrets/index.html",
  title: "Use a scoped secret in either shell mode",
  description:
    "Create a Keychain-backed value, approve its project mapping, compare one-command and ambient loading without ever printing it, and watch trust invalidate the moment the file changes.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Where the value lives", id: "model" },
    { label: "Create the value", id: "create" },
    { label: "Map and approve", id: "map" },
    { label: "One command", id: "run" },
    { label: "Ambient directory", id: "ambient" },
    { label: "Invalidate trust", id: "invalidate" },
    { label: "Deny wins", id: "deny" },
    { label: "Global is not ambient", id: "global" },
    { label: "What a tool can see", id: "tools" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will create a disposable value in macOS Keychain, map it to one project, and exercise both ways Synapse can hand it to something — without the value ever appearing on screen. Then you will break the approval on purpose and watch every path refuse.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and the login Keychain unlocked.</li>
      <li>An empty temporary folder. Do not run this in a real project.</li>
      <li>About twenty-five minutes.</li>
    </ul>

    <h2 id="model">Where the value actually lives</h2>
    <p>Worth being precise about before you start, because the whole design follows from it.</p>
    <table>
      <thead><tr><th>Thing</th><th>Stored where</th></tr></thead>
      <tbody>
        <tr><td>The secret value</td><td>macOS Keychain. Only there, ever.</td></tr>
        <tr><td>Its name, its variable name, its Keychain reference</td><td>Synapse's SQLite database.</td></tr>
        <tr><td>Which variable a folder should get</td><td><code>.synapse.yaml</code> in the project, which holds a reference and never a value.</td></tr>
        <tr><td>Whether that file is trusted</td><td>A SHA-256 digest of its exact bytes, recorded when you approve it.</td></tr>
      </tbody>
    </table>
    <p>No secret value is ever written to SQLite, to YAML, to an MCP response, or to a log. A value is read from Keychain at the moment a child process is launched and goes nowhere else.</p>

    <ol class="steps">
      <li>
        <h3 id="create">Create a vault and a value</h3>
        ${code("shell", `mkdir -p "$HOME/tmp/synapsetutorial"
cd "$HOME/tmp/synapsetutorial"
synapse vault create tutorial
synapse secret set tutorial demo SYNAPSE_TUTORIAL_TOKEN`)}
        <p>Enter a disposable value at the hidden prompt and confirm it. A vault is an organizational label; the three arguments are the vault, the secret's name inside it, and the environment variable it will become.</p>
        ${code("text", `Created vault tutorial
Saved tutorial.demo in Keychain`)}
        <p>Now list what exists. Metadata only — there is no command that prints a value, because there is no reason for one to exist:</p>
        ${code("shell", `synapse secret list tutorial`)}
        ${code("text", `tutorial.demo	SYNAPSE_TUTORIAL_TOKEN	scoped`)}
        ${note("Secrets are never command arguments", "<code>synapse secret set</code> reads from a TTY prompt or stdin, never from an argument. An argument would land in your shell history, in the process table, and in any terminal recording — three places a value should never be.")}
      </li>

      <li>
        <h3 id="map">Map it to this folder and approve the file</h3>
        ${code("shell", `synapse scope init .`)}
        <p>Replace the generated <code>.synapse.yaml</code> with:</p>
        ${code("yaml", `version: 1
scope: project
env:
  SYNAPSE_TUTORIAL_TOKEN: tutorial.demo
deny: []`)}
        <p><code>tutorial.demo</code> is a reference, not a value. This file is safe to commit — that is the point of it holding a reference. Look at the file, then approve its exact bytes:</p>
        ${code("shell", `synapse scope status .
synapse allow
synapse status .`)}
        ${code("text", `Allowed /Users/example/tmp/synapsetutorial/.synapse.yaml`)}
        ${code("text", `Folder: .
Available: SYNAPSE_TUTORIAL_TOKEN
Ambient: ready
/Users/example/tmp/synapsetutorial/.synapse.yaml [project · approved]`)}
        <p><code>Ambient: ready</code> means a shell hook would activate here. It does not mean one is installed.</p>
      </li>

      <li>
        <h3 id="run">Hand it to exactly one command</h3>
        <p>This is the narrower of the two boundaries and the one to prefer. Verify presence without printing content:</p>
        ${code("shell", `synapse run -- sh -c 'test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "tutorial token available"'`)}
        <p>The child prints only the fixed sentence. Confirm the parent shell was never touched:</p>
        ${code("shell", `test -z "$SYNAPSE_TUTORIAL_TOKEN" && echo "parent unchanged"`)}
        <p>Synapse read the Keychain item, set it on that one child, and the child exited with it. Nothing was exported into your session.</p>
        ${note("The child is fully trusted with the value", "Any process launched through <code>synapse run</code> can read and disclose what it is given. This tutorial uses a disposable value and a child that only tests whether it is non-empty. The boundary Synapse enforces is which processes receive a value, not what they do with it afterwards.")}
      </li>

      <li>
        <h3 id="ambient">Activate the whole directory</h3>
        <p>For a permanent setup, use <strong>Settings → Shell environments → Enable shell hook</strong> and open a new terminal. To try it immediately in this shell:</p>
        ${code("shell", `# zsh
eval "$(synapse hook zsh)"

# bash: eval "$(synapse hook bash)"
# fish: synapse hook fish | source`)}
        <p>The approved scope activates at once. Now walk in and out of it:</p>
        ${code("shell", `test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token available"
cd ..
test -z "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token unloaded"
cd synapsetutorial
test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token restored"`)}
        <p>All three print. Leaving the directory unloads exactly the variables the hook loaded — and if one of them had a value <em>before</em> activation, leaving restores that original value rather than unsetting it. The hook tracks which keys it owns precisely so it can put things back as it found them.</p>
        ${note("The entire shell is trusted now", "Every process launched from this shell can read the value, including ones you did not think about. Prefer <code>synapse run</code> for anything sensitive, and keep shell tracing off while the hook evaluates — a traced shell prints what it is setting.")}
      </li>

      <li>
        <h3 id="invalidate">Break the approval and watch everything refuse</h3>
        <p>Approval is bound to the exact bytes of the file. A blank line is a different file:</p>
        ${code("shell", `echo "" >> .synapse.yaml
synapse status .`)}
        ${code("text", `Folder: .
Available: none
Ambient: blocked
/Users/example/tmp/synapsetutorial/.synapse.yaml [project · pending]
warning: /Users/example/tmp/synapsetutorial/.synapse.yaml: Scope has not been approved`)}
        <p>Three things happen at once. Ambient mode unloads at the next prompt. <code>synapse run</code> refuses before launching anything. And <code>synapse launch</code> refuses to start a coding tool at all:</p>
        ${code("shell", `synapse run -- sh -c 'echo should-not-run'`)}
        ${code("text", `Error: vault scope is not ready:
/Users/example/tmp/synapsetutorial/.synapse.yaml: Scope has not been approved`)}
        <p>It refuses rather than running with the variable missing, because a command that runs against a half-resolved environment fails in ways that look like success. Read what changed, then re-approve:</p>
        ${code("shell", `synapse scope status .
synapse allow`)}
        <p>Both presence tests work again. This is what makes a committed <code>.synapse.yaml</code> safe: a teammate's edit, or a malicious one, arrives untrusted and stays that way until a human looks at it.</p>
      </li>

      <li>
        <h3 id="deny">Prove that deny wins</h3>
        <p>Scopes resolve from the root of your filesystem down to the current folder, so a narrower file can add variables. It can never add back one a broader scope denied. Move the variable into <code>deny</code>:</p>
        ${code("yaml", `version: 1
scope: project
env: {}
deny:
  - SYNAPSE_TUTORIAL_TOKEN`)}
        ${code("shell", `synapse allow
synapse status .`)}
        <p><code>Available</code> no longer lists the name. Create a folder-level scope beneath this one that tries to map it again, approve that too, and it still will not appear. Deny is a ceiling, not a default.</p>
      </li>

      <li>
        <h3 id="global">See why global is not the same as ambient</h3>
        <p>A secret can be marked global, meaning it is available without any scope file naming it:</p>
        ${code("shell", `synapse secret global tutorial.demo on
synapse secret list tutorial`)}
        ${code("text", `tutorial.demo is now global`)}
        ${code("text", `tutorial.demo	SYNAPSE_TUTORIAL_TOKEN	global`)}
        <p>Now move to a folder with no <code>.synapse.yaml</code> at all and check:</p>
        ${code("shell", `cd ~ && synapse status .`)}
        ${code("text", `Folder: .
Available: SYNAPSE_TUTORIAL_TOKEN
Ambient: inactive`)}
        <p>Read those two lines together, because the difference between them is a deliberate safety property. The name is <em>available</em> — <code>synapse run</code> here would provide it. But ambient is <strong>inactive</strong>: a shell hook will not load it, because an ambient environment never activates from a global mapping alone. Turning on a global secret cannot silently populate every shell you open.</p>
        ${code("shell", `synapse secret global tutorial.demo off`)}
      </li>

      <li>
        <h3 id="tools">Check what a connected tool can actually see</h3>
        <p>Go back to the tutorial folder and ask a connected coding tool to call <code>vaultstatus</code>. It reports the variable names available here, the trust state of each scope, and whether ambient mode is ready — and no values, because that tool has no code path that reads one.</p>
        <p>The same is true of a preview. <code>synapse launch claude --print</code> shows <code>DEMO_TOKEN=&lt;from keychain&gt;</code> rather than a redacted value, because a preview calls a different function that lists names and never opens the Keychain at all. There is no value in the process to leak.</p>
      </li>
    </ol>

    <h2 id="cleanup">Clean up</h2>
    ${code("shell", `synapse deny
synapse secret forget tutorial.demo
synapse vault delete tutorial
cd .. && rm -rf synapsetutorial`)}
    ${code("text", `Forgot tutorial.demo
Deleted vault tutorial`)}
    <p>The next prompt unloads the ambient value. Forgetting the secret removes both the Keychain item and Synapse's record of it — this is the one cleanup step that matters, because uninstalling Synapse later would remove its knowledge of the item without removing the item itself.</p>

    <h2>What you can rely on</h2>
    <ul>
      <li>Secret values never reach SQLite, YAML, an MCP response, or a log.</li>
      <li>Secrets are never accepted as command arguments — only from a TTY prompt or stdin.</li>
      <li>An ambient shell never activates from a global mapping alone, nor from an unapproved, changed, or incomplete scope.</li>
      <li>Unloading restores values that existed before activation rather than unsetting them.</li>
      <li>Any scope warning refuses <code>run</code> and <code>launch</code> outright rather than proceeding with part of the environment.</li>
    </ul>

    <h2>Next step</h2>
    <p>Continue with <a href="../skills/">Keep one skill library across every tool</a> to finish the daily driver level.</p>
  `,
};

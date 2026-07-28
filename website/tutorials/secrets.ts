import { code, note } from "../markup";
import type { Page } from "../types";

export const secrets: Page = {
  path: "tutorials/secrets/index.html",
  title: "Use a scoped secret in either shell mode",
  description: "Create a Keychain-backed value, approve its project mapping, compare one-command and ambient loading, and observe trust invalidation.",
  kind: "tutorial",
  toc: [
    { label: "Create the value", id: "create" },
    { label: "Map and approve", id: "map" },
    { label: "One command", id: "run" },
    { label: "Ambient directory", id: "ambient" },
    { label: "Test invalidation", id: "invalidate" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2>Outcome and prerequisites</h2>
    <p>You will create a harmless tutorial value in macOS Keychain, map it to <code>SYNAPSE_TUTORIAL_TOKEN</code> for one temporary project, approve the exact YAML, and test both environment boundaries without revealing content.</p>
    <p>Run this tutorial in an empty temporary folder. The CLI must be installed and the login Keychain unlocked.</p>

    <ol class="steps">
      <li>
        <h3 id="create">Create a vault and value</h3>
        ${code("shell", `mkdir -p "$HOME/tmp/synapsetutorial"
cd "$HOME/tmp/synapsetutorial"
synapse vault create tutorial
synapse secret set tutorial demo SYNAPSE_TUTORIAL_TOKEN`)}
        <p>Enter a disposable value at the hidden prompt and confirm it. The command reports <code>Saved tutorial.demo in Keychain</code>. List metadata without revealing the value:</p>
        ${code("shell", `synapse secret list tutorial`)}
      </li>
      <li>
        <h3 id="map">Create and edit the scope</h3>
        ${code("shell", `synapse scope init .`)}
        <p>Replace <code>.synapse.yaml</code> with:</p>
        ${code("yaml", `version: 1
scope: project
env:
  SYNAPSE_TUTORIAL_TOKEN: tutorial.demo
deny: []`)}
        <p>Inspect the file, then approve its exact bytes:</p>
        ${code("shell", `synapse scope status .
synapse allow
synapse status .`)}
        <p>Status should list <code>SYNAPSE_TUTORIAL_TOKEN</code> as available, the scope as approved, and ambient activation as ready.</p>
      </li>
      <li>
        <h3 id="run">Verify presence without printing content</h3>
        ${code("shell", `synapse run -- sh -c 'test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "tutorial token available"'`)}
        <p>The child prints only the fixed success sentence. Synapse read the Keychain item and set it on that child; the parent shell remains unchanged.</p>
        ${code("shell", `test -z "$SYNAPSE_TUTORIAL_TOKEN" && echo "parent unchanged"`)}
        ${note("The child is trusted with plaintext", "Any process launched through synapse run can read and disclose the provided values. This tutorial uses a disposable value and a child that tests only whether it is non-empty.")}
      </li>
      <li>
        <h3 id="ambient">Activate the directory automatically</h3>
        <p>For persistent setup, open <strong>Settings → Shell environments</strong>, choose <strong>Enable shell hook</strong>, then open a new terminal and return to the tutorial folder. To continue immediately in the current shell, evaluate the matching hook manually:</p>
        ${code("shell", `# zsh
eval "$(synapse hook zsh)"

# bash: eval "$(synapse hook bash)"
# fish: synapse hook fish | source`)}
        <p>The current approved scope activates immediately. Confirm presence with a fixed message, then leave and reenter the project:</p>
        ${code("shell", `test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token available"
cd ..
test -z "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token unloaded"
cd synapsetutorial
test -n "$SYNAPSE_TUTORIAL_TOKEN" && echo "ambient token restored"`)}
        <p>If the variable had a value before activation, leaving restores that original value instead of unsetting it.</p>
        ${note("The whole activated shell is trusted", "Every process launched from this shell can read the ambient value. Prefer synapse run for a sensitive one-off command. Keep shell tracing disabled while the hook evaluates changes so plaintext values are not printed.")}
      </li>
      <li>
        <h3 id="invalidate">Observe trust invalidation</h3>
        <p>Add a blank line to <code>.synapse.yaml</code>, then run:</p>
        ${code("shell", `synapse status .
synapse run -- sh -c 'echo should-not-run'`)}
        <p>Status should report the scope as changed, ambient mode should unload at the next prompt, and the second command should refuse before launching the shell. Inspect the new content, run <code>synapse allow</code>, and verify both safe presence tests work again.</p>
      </li>
      <li>
        <h3>Test a permanent deny</h3>
        <p>Add <code>SYNAPSE_TUTORIAL_TOKEN</code> to <code>deny</code>, remove it from <code>env</code>, approve the file with <code>synapse allow</code>, and run status. The name should no longer be available. A narrower folder scope cannot add back a name denied by this project scope.</p>
      </li>
    </ol>

    <h2 id="cleanup">Clean up</h2>
    ${code("shell", `synapse deny
synapse secret forget tutorial.demo
synapse vault delete tutorial`)}
    <p>The next prompt unloads the ambient value. Delete the temporary folder when you no longer need its YAML. Forgetting the secret removes both the Keychain item and Synapse metadata. Deleting the now-empty vault removes its organizational label.</p>
  `,
};

import { code, note } from "../markup";
import type { Page } from "../types";

export const continuity: Page = {
  path: "tutorials/continuity/index.html",
  title: "Carry one decision between tools",
  description: "Make a real project convention durable in one session, recover it before work in another, and correct it from the source of truth.",
  kind: "tutorial",
  toc: [
    { label: "Choose a decision", id: "choose" },
    { label: "Remember", id: "remember" },
    { label: "Recall elsewhere", id: "recall" },
    { label: "Inspect and correct", id: "correct" },
    { label: "Verify", id: "verify" },
  ],
  body: `
    <h2>Outcome and prerequisites</h2>
    <p>You will prove that durable context survives both a session boundary and a tool boundary. Complete <a href="../connect/">the connection tutorial</a> first. Two connected tools make the handoff visible, but two separate sessions of one tool also prove persistence.</p>

    <ol class="steps">
      <li>
        <h3 id="choose">Choose one confirmed decision</h3>
        <p>Use something concrete and harmless that should affect future work. Good examples are a package-manager choice, a supported deployment target, a naming rule, or a correction to a previously wrong assumption.</p>
        ${note("Do not stage a fake secret", "Memory is plain text available to connected tools. Use an ordinary project convention, never a token, credential, or private key.")}
      </li>
      <li>
        <h3 id="remember">Confirm and remember it</h3>
        <p>In the first connected tool, establish the decision explicitly, then ask it to remember the confirmed result with the repository name as the source. Example:</p>
        <blockquote>We have confirmed that this repository uses Bun for JavaScript tasks. Remember that durable convention with source synapstutorial.</blockquote>
        <p>The tool should call <code>remember</code> once and return an ID. If it stores a transcript instead of the concise decision, correct the entry in the next step.</p>
      </li>
      <li>
        <h3>Close the session</h3>
        <p>End the first tool session completely. This rules out conversational context and leaves the local Synaps database as the continuity layer.</p>
      </li>
      <li>
        <h3 id="recall">Recall from another tool</h3>
        <p>Open the second connected tool in the same repository. Before telling it the convention, ask:</p>
        <blockquote>Recall the confirmed JavaScript tooling convention for this project before proposing commands.</blockquote>
        <p>It should call <code>recall</code>, recover the Bun convention, and use that context in its answer. An explicit request is useful for this tutorial; normal global instructions also tell the tool to recall before history-dependent decisions.</p>
      </li>
      <li>
        <h3 id="correct">Inspect the source of truth</h3>
        ${code("shell", `synaps memory list "Bun JavaScript"
synaps memory show <id>`)}
        <p>Open the same record in the Memories screen. Confirm that the exact text is concise, stable, and correctly sourced. If not, replace it:</p>
        ${code("shell", `printf '%s\n' 'Use Bun for JavaScript tasks in this repository.' \\
  | synaps memory edit <id> synapstutorial`)}
      </li>
      <li>
        <h3 id="verify">Verify the correction</h3>
        <p>Start one more new session and recall the convention again. The response should contain the corrected text, proving that editing changes the durable source rather than creating another conflicting record.</p>
      </li>
    </ol>

    <h2>Keep or clean up</h2>
    <p>If the convention is real, keep it. If it was tutorial-only, remove the exact ID with <code>synaps memory delete &lt;id&gt; --confirm</code>. Avoid a full wipe unless you intend to remove every memory.</p>
  `,
};

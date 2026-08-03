import { code, note } from "../markup";
import type { Page } from "../types";

export const continuity: Page = {
  path: "tutorials/continuity/index.html",
  title: "Carry one decision between tools",
  description:
    "Make a real project convention durable in one session, recover it in another tool after closing the first, correct it at the source, and see the scope rule keep other projects out.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Choose a decision", id: "choose" },
    { label: "Remember it", id: "remember" },
    { label: "Close the session", id: "close" },
    { label: "Recall elsewhere", id: "recall" },
    { label: "Inspect and correct", id: "correct" },
    { label: "Prove the scope rule", id: "scope" },
    { label: "Global versus project", id: "global" },
    { label: "Keep or clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will prove that durable context survives two boundaries a conversation cannot: the end of a session, and the move to a different tool. Then you will correct it at the source and confirm that another project cannot see it.</p>
    <ul>
      <li><a href="../connect/">Install and connect your first tools</a> completed.</li>
      <li>Two connected tools make the handoff visible. Two separate sessions of one tool prove persistence just as well.</li>
      <li>A real project folder, and a second one for the scope check.</li>
    </ul>

    <ol class="steps">
      <li>
        <h3 id="choose">Choose one decision worth keeping</h3>
        <p>Use something concrete, harmless, and genuinely load-bearing for future work. A package-manager choice, a supported deployment target, a naming rule, or a correction to an assumption that turned out wrong are all good. The test of a good memory is whether a session that did not know it would do something different.</p>
        <table>
          <thead><tr><th>Worth remembering</th><th>Not worth remembering</th></tr></thead>
          <tbody>
            <tr><td>"Use Bun for JavaScript tasks in this repository."</td><td>"The user asked me to check the tests."</td></tr>
            <tr><td>"Releases are triggered by a version bump on main."</td><td>"The build is currently failing."</td></tr>
            <tr><td>"Do not hand-edit the generated site directory."</td><td>A full transcript of how you reached the decision.</td></tr>
          </tbody>
        </table>
        <p>The right-hand column is transient. It is true today and misleading next week, and a memory store full of it makes recall worse rather than better.</p>
        ${note("Never stage a fake secret", "Memory is plain text available to every connected tool. Use an ordinary project convention, never a token, credential, or private key — not even a made-up one, because a habit formed here is a habit that shows up in a real store.")}
      </li>

      <li>
        <h3 id="remember">Confirm it, then have the tool remember it</h3>
        <p>In the first connected tool, establish the decision explicitly, then ask for it to be stored:</p>
        <blockquote>We have confirmed that this repository uses Bun for JavaScript tasks. Remember that durable convention with source synapsetutorial.</blockquote>
        <p>The tool should call <code>remember</code> once, with project scope and this repository's root, and return an ID. Two things can go wrong here and both are worth noticing:</p>
        <ul>
          <li><strong>It stores a transcript instead of the decision.</strong> A memory should be one durable idea in one or two sentences. You will fix this in the correction step.</li>
          <li><strong>It stores global scope instead of project.</strong> Global memory is returned in every project, which is right for a preference about how you like to work and wrong for a fact about one repository.</li>
        </ul>
      </li>

      <li>
        <h3 id="close">Close the session completely</h3>
        <p>Exit the first tool. Not a new conversation inside it — exit it. This is what rules out conversational context and leaves the local database as the only thing carrying the decision forward.</p>
      </li>

      <li>
        <h3 id="recall">Recall it from the other tool</h3>
        <p>Open the second connected tool in the same repository. If it is Claude Code, look at what happens before you type anything: the session hook has already recalled this project's memory and handed it over, so the convention is in context from the first turn. That is the difference between memory that works and memory that depends on the model remembering to ask.</p>
        <p>Either way, ask a question the convention should change the answer to:</p>
        <blockquote>What command should I use to install dependencies here?</blockquote>
        <p>It should answer with Bun rather than npm, and be able to say where that came from. If it guesses wrong, ask it explicitly to recall the JavaScript tooling convention for this project — that tells you whether the memory is missing or merely was not consulted.</p>
      </li>

      <li>
        <h3 id="correct">Inspect the source of truth and correct it</h3>
        <p>What the tool reported is a rendering. Go and look at the record itself:</p>
        ${code("shell", `synapse memory list "Bun JavaScript"`)}
        ${code("text", `4	project:/Users/example/project	synapsetutorial	We confirmed together that this repository uses Bun for JavaScript tasks rather than npm, after discussing…`)}
        ${code("shell", `synapse memory show 4`)}
        <p>If the body is a paragraph of conversation rather than a durable statement, replace it. Editing keeps the same ID, so nothing else has to change and no contradicting second record is created:</p>
        ${code("shell", `printf '%s\\n' 'Use Bun for JavaScript tasks in this repository.' \\
  | synapse memory edit 4 synapsetutorial`)}
        ${code("text", `Updated memory #4`)}
        <p>Start one more new session and ask again. The answer should now reflect the corrected text — which proves that editing changed the durable source rather than layering a correction on top of a wrong record.</p>
        ${note("Correcting beats appending", "Two memories that disagree are worse than one that is slightly wrong, because recall returns both and the model has to guess which is current. When a convention changes, edit the record. Add a new one only when it is genuinely a new fact.")}
      </li>

      <li>
        <h3 id="scope">Prove other projects cannot see it</h3>
        <p>This is the property that makes a shared store usable. Open a session in an unrelated project and ask the same question:</p>
        <blockquote>What command should I use to install dependencies here?</blockquote>
        <p>It must not answer with the first project's convention. Confirm from the terminal that the record is scoped where you think it is:</p>
        ${code("shell", `synapse memory show 4`)}
        ${code("text", `Memory #4
Scope: project
Project: /Users/example/project
Source: synapsetutorial`)}
        <p>Recall from any project returns everything global plus everything stored for <em>that</em> project. Another project's memory never enters the response. The project root is resolved by walking up for a <code>.git</code> directory or a <code>.synapse.yaml</code>, so a session opened in a subdirectory still resolves to the same root — which is why the same memory is found whether you start at the repository top or three folders down.</p>
      </li>

      <li>
        <h3 id="global">Store one thing globally, and see the difference</h3>
        <p>Some things belong everywhere. A preference about how you like to work is not a fact about one repository:</p>
        ${code("shell", `printf '%s\\n' 'Prefer small, focused commits with imperative subject lines.' \\
  | synapse memory add synapsetutorial --global`)}
        ${code("text", `Stored memory #5`)}
        <p>Now recall from both projects. The global preference appears in each; the Bun convention appears in only one. That split — global plus this project, never another project — is the entire scope model, and it is enforced in the query rather than trusted to the caller.</p>
      </li>
    </ol>

    <h2 id="cleanup">Keep or clean up</h2>
    <p>If the convention is real, keep it — you have just made your tools better at this repository. If it was tutorial-only, remove the exact IDs:</p>
    ${code("shell", `synapse memory list synapsetutorial
synapse memory delete 4 --confirm
synapse memory delete 5 --confirm`)}
    <p>Deleting requires <code>--confirm</code> and names the record it is about to remove. Avoid <code>synapse memory wipe</code> unless you genuinely intend to remove every memory in the store; it exists for starting over, not for tidying.</p>

    <h2>Next step</h2>
    <p>You have finished the newcomer level. Continue with <a href="../curate/">Curate and optimize memory</a>, which is about keeping a store useful once it has more than a handful of entries in it.</p>
  `,
};

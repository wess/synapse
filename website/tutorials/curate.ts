import { code, note } from "../markup";
import type { Page } from "../types";

export const curate: Page = {
  path: "tutorials/curate/index.html",
  title: "Curate and optimize memory",
  description:
    "Build a small memory set, learn how search actually behaves, correct entries at the source, tune the recall budget, import existing notes reversibly, and remove only what you added.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Add entries", id: "add" },
    { label: "Search and inspect", id: "search" },
    { label: "How search behaves", id: "behavior" },
    { label: "Correct at the source", id: "correct" },
    { label: "Tune the budget", id: "budgets" },
    { label: "Import existing notes", id: "import" },
    { label: "Undo an import", id: "undo" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will build a small set of entries, understand why search returns what it returns, correct one at the source, compare what each recall budget actually sends to a model, import a Markdown file reversibly, and then undo it. This is the tutorial to read when recall is returning too much, too little, or the wrong thing.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and at least one tool connected.</li>
      <li>A project folder. Steps here create and delete entries under one source label.</li>
    </ul>
    ${note("Write down the IDs", "Every add prints its new numeric ID. Keep those numbers for the cleanup step. Do not reach for <code>synapse memory wipe</code> in a store that has real context in it — it removes everything, and it is meant for starting over rather than tidying.")}

    <ol class="steps">
      <li>
        <h3 id="add">Add focused entries</h3>
        <p>Content is read from stdin, never from an argument, so a memory can contain anything without shell quoting getting in the way:</p>
        ${code("shell", `printf '%s\\n' 'Tutorial apps use port 4100.' | synapse memory add synapsetutorial
printf '%s\\n' 'Tutorial commands use Bun.' | synapse memory add synapsetutorial
printf '%s\\n' 'Tutorial backups go in the backups folder.' | synapse memory add synapsetutorial`)}
        ${code("text", `Stored memory #1
Stored memory #2
Stored memory #3`)}
        <p><code>synapsetutorial</code> is the <em>source</em> — a label saying where the fact came from. Give every memory one. It is what lets you find a set later, audit what a particular session stored, and delete a batch without hunting.</p>
        <p>One durable idea per entry. Three entries about ports, runtimes, and backups can each be recalled, corrected, and deleted on their own; one entry containing all three is returned whole every time any part of it is relevant, and cannot be corrected without rewriting the rest.</p>
      </li>

      <li>
        <h3 id="search">Search and inspect</h3>
        ${code("shell", `synapse memory list tutorial`)}
        ${code("text", `2	project:/Users/example/project	synapsetutorial	Tutorial commands use Bun.
1	project:/Users/example/project	synapsetutorial	Tutorial apps use port 4100.`)}
        <p>The columns are the ID, the scope with its project, the source, and the body. For anything scripted, ask for JSON instead:</p>
        ${code("shell", `synapse memory list "Bun" --json`)}
        ${code("text", `[
  {
    "id": 2,
    "body": "Tutorial commands use Bun.",
    "source": "synapsetutorial",
    "scope": "project",
    "project": "/Users/example/project",
    "created": 1785776940
  }
]`)}
        <p>And for one exact record, with nothing trimmed:</p>
        ${code("shell", `synapse memory show 2`)}
        ${code("text", `Memory #2
Scope: project
Project: /Users/example/project
Source: synapsetutorial
Created: 1785776940

Tutorial commands use Bun.`)}
      </li>

      <li>
        <h3 id="behavior">Understand how search behaves</h3>
        <p>Recall is full-text search with two behaviors worth knowing, because both explain results that otherwise look wrong.</p>
        <p><strong>Common words are dropped.</strong> A query is stripped of words that carry no meaning on their own. Try it:</p>
        ${code("shell", `synapse memory list "what are the"`)}
        <p>Nothing in that query survives, so instead of matching every memory containing <em>the</em>, Synapse answers the way it answers an empty query — with the most recent entries. That is deliberate. Matching on a common word is both wrong, because a memory that happens to contain <em>are</em> is not the answer to anything, and slow, because a common word makes the index rank most of the store.</p>
        <p><strong>An empty query is a real question.</strong> It means "what should I know here", and it returns the most recent memory in scope. This is exactly what a session opening asks, and it is what the Claude Code session hook uses.</p>
        ${code("shell", `synapse memory list`)}
        <p><strong>Words that carry a preference are kept.</strong> <code>not</code>, <code>never</code>, and <code>use</code> look like function words but change the meaning of a convention completely, so they stay in the query. "Never use npm" and "use npm" must not search identically.</p>
        <p><strong>You can ask what it did.</strong> When a memory you know is stored does not come back, the fastest answer is to make the search show its working:</p>
        ${code("shell", `synapse memory list "what are the credentials" --explain`)}
        ${code("text", `Query:      what are the credentials
Mode:       search
Expression: "credentials"
Searched:   credentials
Dropped:    what, are, the (matches nearly every memory)
Matches:    0`)}
        <p>That distinguishes the two things that look identical from the outside: a query that lost its only real word, and a store that genuinely holds nothing.</p>
        <p><strong>And you can skip the ranking entirely.</strong> When you want an exact string — a flag, an identifier, a path — the ranked search is the wrong tool, because the thing you are looking for may be a word it drops:</p>
        ${code("shell", `synapse memory grep -- --no-verify`)}
        ${note("Scope is applied in the query, not after it", "Every search returns everything global plus everything for the current project, and nothing from any other project. That is enforced where the rows are selected rather than filtered afterwards, so there is no path where another project's memory is read and then discarded.")}
      </li>

      <li>
        <h3 id="correct">Correct at the source</h3>
        <p>A convention becomes more precise. Replace the record rather than adding a second one that disagrees with it:</p>
        ${code("shell", `printf '%s\\n' 'Tutorial commands use Bun unless a task requires another runtime.' \\
  | synapse memory edit 2 synapsetutorial
synapse memory show 2`)}
        ${code("text", `Updated memory #2`)}
        <p>The ID is stable, so anything referring to this memory still refers to the right thing, and future recall returns only the corrected text. Two entries that contradict each other are the single most common way a memory store gets worse as it grows: recall returns both and the model has to guess which is current.</p>
        <p>Editing is right when the old wording had nothing worth keeping. When the old version was <em>true and then stopped being true</em>, supersede it instead — the new memory becomes the one recall returns, and the old one stays readable as the record of what used to be the case:</p>
        ${code("shell", `printf '%s\\n' 'Tutorial commands use Bun. Node is used only for the release script.' \\
  | synapse memory add synapsetutorial
synapse memory supersede 2 6`)}
        ${code("text", `Memory #2 superseded by #6; recall now returns #6 instead
Undo with: synapse memory restore 2`)}
        <p>Check both. Memory 2 is still there, marked, and out of recall; memory 6 is what a tool now sees. If you decide you were wrong, <code>synapse memory restore 2</code> puts it straight back.</p>
        ${code("shell", `synapse memory show 2
synapse memory list`)}
      </li>

      <li>
        <h3 id="budgets">Tune what recall actually sends</h3>
        <p>The budget controls how much a <code>recall</code> response costs a model in context. Look at where you are:</p>
        ${code("shell", `synapse settings show`)}
        ${code("text", `optimization	balanced
result limit	8
character budget	6000
mesh	off
shell modes	command-scoped, ambient directory
zsh hook	eval "$(synapse hook zsh)"`)}
        <table>
          <thead><tr><th>Budget</th><th>Results</th><th>Characters</th><th>When to use it</th></tr></thead>
          <tbody>
            <tr><td>Full</td><td>25</td><td>unbounded</td><td>A small store, or when you want everything and context is cheap.</td></tr>
            <tr><td>Balanced</td><td>8</td><td>6,000</td><td>The default, and the right answer for most stores.</td></tr>
            <tr><td>Lean</td><td>4</td><td>2,800</td><td>A large store, a long session, or a model whose context you are protecting.</td></tr>
          </tbody>
        </table>
        ${code("shell", `synapse settings optimize lean`)}
        ${code("text", `Recall optimization set to Lean`)}
        <p>Ask a connected tool to recall <code>synapsetutorial</code> and note how many entries come back. Switch to <code>full</code> and ask again. The stored bodies never change — <code>synapse memory show</code> returns the same text at every setting. The budget only shapes the response.</p>
        <p>A tool can request a smaller budget for one call, and that request can only ever shrink your configured ceiling. A session cannot talk its way into a larger response than you have allowed.</p>
        ${code("shell", `synapse settings optimize balanced`)}
      </li>

      <li>
        <h3 id="import">Import notes you already have</h3>
        <p>If you have been keeping conventions in a Markdown file, bring them in. Every import previews first:</p>
        ${code("shell", `synapse memory import markdown notes.md`)}
        ${code("text", `Markdown: 1 found · 1 ready · 0 existing · 0 flagged
ready	notes.md	# Conventions - Use Bun, never npm. - Deploys target Apple silicon only. …
Preview only. Add --confirm to import safe entries.`)}
        <p>Now try one with a credential in it and watch what happens:</p>
        ${code("text", `Markdown: 1 found · 0 ready · 0 existing · 1 flagged
review	flagged.md	Content hidden until the source file is reviewed.
	warning: mentions a credential-shaped environment variable
Preview only. Add --confirm to import safe entries.`)}
        <p>Flagged content is not merely skipped — it is not even displayed, because printing it to your terminal to warn you about it would defeat the point. <code>--confirm</code> imports the ready entries and leaves flagged ones untouched.</p>
        ${note("The flagging is conservative, not exhaustive", "It looks for private-key markers, credential-shaped variable names, assignments to password- and token-like keys, and bearer tokens. A secret that matches none of those patterns will not be caught. Read your own preview — the flag is a safety net, not a substitute for looking.")}
        ${code("shell", `synapse memory import markdown notes.md --confirm`)}
        ${code("text", `Import batch #1: 1 stored, 0 linked, 0 already imported, 0 flagged and left untouched
Undo with: synapse memory undo 1 --confirm`)}
        <p>Imports are idempotent. Running the same file again reports the entries as already imported rather than creating duplicates, so a repeated import is harmless. Claude and Codex memory stores import the same way with <code>synapse memory import claude</code> and <code>synapse memory import codex</code>.</p>
      </li>

      <li>
        <h3 id="undo">Undo the whole batch</h3>
        ${code("shell", `synapse memory imports`)}
        ${code("text", `1	markdown	1 stored	0 linked	active`)}
        ${code("shell", `synapse memory undo 1 --confirm
synapse memory imports`)}
        ${code("text", `Undid import batch #1; removed 1 imported memories`)}
        ${code("text", `1	markdown	1 stored	0 linked	undone`)}
        <p>The batch is recorded rather than forgotten, so an import you regret is one command to reverse and the record of it having happened survives. Anything you wrote by hand is untouched — undo removes only what that batch stored.</p>
      </li>
    </ol>

    <h2 id="cleanup">Delete only what you added</h2>
    ${code("shell", `synapse memory list synapsetutorial
synapse memory delete 1 --confirm
synapse memory delete 2 --confirm
synapse memory delete 3 --confirm`)}
    ${code("text", `Deleted memory #1`)}
    <p>Both destructive commands refuse without <code>--confirm</code>, and say which record they mean:</p>
    ${code("text", `Error: add --confirm to delete memory #3
Error: add --confirm to delete every memory`)}
    <p>A final <code>synapse memory list synapsetutorial</code> should return nothing. Use the <strong>Memories</strong> screen for a visual review before deleting anything you did not create in this tutorial.</p>

    <h2>Next step</h2>
    <p>Continue with <a href="../secrets/">Use a scoped secret in either shell mode</a>, which is the other half of what Synapse hands a tool — and the half where the boundaries matter most.</p>
  `,
};

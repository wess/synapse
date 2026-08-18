import { code, note } from "../markup";
import type { Page } from "../types";

export const memory: Page = {
  path: "docs/memory/index.html",
  title: "Memory and recall",
  description: "Know what becomes durable, how search works, how to correct stored context, and what each response budget changes.",
  kind: "docs",
  toc: [
    { label: "Durable memory", id: "durable" },
    { label: "Global and project scope", id: "scope" },
    { label: "Import existing memory", id: "import" },
    { label: "Search and recall", id: "recall" },
    { label: "Inspect and correct", id: "control" },
    { label: "Correcting a memory", id: "supersede" },
    { label: "Response budgets", id: "budgets" },
    { label: "Destructive actions", id: "destructive" },
  ],
  body: `
    <h2 id="durable">What belongs in durable memory</h2>
    <p>Good memory remains useful after the current conversation ends. Store confirmed decisions, recurring preferences, corrected assumptions, naming conventions, project constraints, and facts whose source is clear.</p>
    <p>Shared guidance lives in <code>SOUL.md</code>. Each connected tool's global instruction file contains a managed pointer to it, and the MCP server loads the same file during initialization. Both tools therefore receive the same recall, storage, scope, and token-budget policy.</p>
    <p>Do not store transient task status, speculative ideas, full conversation transcripts, secret values, or instructions that attempt to override the current request or repository guidance. Synapse is the canonical durable memory store; it does not create separate Markdown files for individual memories.</p>
    ${code("shell", `printf '%s\n' 'Use Bun for JavaScript tasks in this repository.' \\
  | synapse memory add tooling`)}
    <p><code>memory add</code> and <code>memory edit</code> read the body from stdin. This keeps long content and shell metacharacters out of command arguments. Empty or whitespace-only memory is rejected.</p>

    <h2 id="scope">Global and project scope</h2>
    <p>Project memory is the default. Synapse resolves the supplied path to its repository or approved project root and returns that memory only alongside global memory for the same project. Unrelated project memory stays out of MCP recall.</p>
    <p>Use global scope only for preferences and conventions that should follow you everywhere. The desktop editor can move a record between scopes. The CLI accepts an explicit scope when adding:</p>
${code("shell", `printf '%s\n' 'Use Bun in this repository.' | synapse memory add tooling --project .
printf '%s\n' 'Prefer concise status updates.' | synapse memory add preferences --global`)}

    <h2 id="import">Import existing memory</h2>
    <p>The Memory screen previews Claude and Codex independently, maps each item to its project when possible, and imports safe entries into the same scoped store. Source files are read-only: Synapse never edits or deletes them.</p>
    <ul>
      <li>Claude imports leaf Markdown files from project memory directories and skips <code>MEMORY.md</code> indexes.</li>
      <li>Codex imports processed memory rows from a recognized local memory database and maps thread IDs to project roots through the local thread catalog.</li>
      <li>Full conversations, session history, tasks, settings, authentication files, and global instruction files are not imported.</li>
      <li>Credential-shaped entries are held for review. Their content is hidden in previews and is not imported by the app.</li>
      <li>Repeated imports are idempotent. Exact scoped duplicates are linked instead of copied, and each batch can be undone without removing a memory that was edited or shared by another source.</li>
    </ul>
${code("shell", `synapse memory import claude
synapse memory import claude --confirm
synapse memory import codex --confirm
synapse memory import markdown ./notes --confirm
synapse memory imports
synapse memory undo <batch> --confirm`)}
${note("Review before overriding the guard", "The CLI can include flagged entries only with both --include-flagged and --confirm. Review the named source file first; memory is plain text and available to connected tools.")}

    <h2 id="recall">Search and recall</h2>
    <p>Memory is stored in an SQLite FTS5 table using Unicode tokenization. A non-empty query searches the memory body; an empty query returns recent entries. Connected tools are instructed to begin with a focused query, the smallest useful result limit, and a Lean response.</p>
${code("json", `{
  "query": "project naming conventions",
  "limit": 4,
  "budget": "lean",
  "project": "/Users/example/project"
}`)}
    <p>The optional per-call <code>budget</code> may be <code>full</code>, <code>balanced</code>, or <code>lean</code>. It can only make the response smaller than the mode selected in Settings; a tool cannot override a user-selected Lean ceiling with Full.</p>
    ${code("shell", `synapse memory list "Bun JavaScript"
synapse memory list --json
synapse memory show 24
synapse memory show 24 --json`)}
    <p>The CLI lists up to 100 matching memories across the local store for management. Each record contains an integer ID, exact body, source, global or project scope, project root, and Unix timestamp. JSON output is suitable for local automation.</p>
    <p>Ranked search drops words that appear in nearly every memory \u2014 <em>the</em>, <em>are</em>, <em>where</em> \u2014 because a memory that matches only on one of those is a confident wrong answer, and an agent acts on it. Words that carry meaning in a preference, including <em>not</em>, <em>never</em>, and <em>use</em>, are kept. Two commands exist for when that is the wrong behaviour or the wrong result:</p>
    ${code("shell", `synapse memory grep -- --no-verify
synapse memory list where are the credentials --explain`)}
    <p><code>memory grep</code> matches the characters you give it and nothing else, which is what you want for an identifier, a flag, a path, or a word the ranked search treats as noise. <code>--explain</code> answers the other question \u2014 why a memory you know is stored did not come back:</p>
    ${code("text", `Query:      where are the credentials
Mode:       search
Expression: "credentials"
Searched:   credentials
Dropped:    where, are, the (matches nearly every memory)
Matches:    1

-0.4596	31	global	vault	Credentials live in Keychain, never in the repository.`)}

    <h2 id="control">Inspect and correct</h2>
    <p>The desktop <strong>Memories</strong> screen searches the same store and lets you inspect, edit, or delete individual entries. CLI edits replace the body and optionally the source:</p>
    ${code("shell", `printf '%s\n' 'Use Bun unless a task explicitly requires another runtime.' \\
  | synapse memory edit 24 tooling

synapse memory show 24`)}
    <p>Editing changes the stored original. Recall optimization does not.</p>

    <h2 id="supersede">Correcting a memory</h2>
    <p>A convention changes. Something you stored last month is now wrong. Adding the new version on its own leaves two memories contradicting each other, both are recalled, and the ranking decides which one a tool acts on \u2014 possibly the one you had already retracted.</p>
    <p>Say which replaced which:</p>
    ${code("shell", `printf '%s\n' 'Deploys run from the release branch, after the tag is signed.' \\
  | synapse memory add deploys --global

synapse memory supersede 12 47`)}
    <p>A connected tool does it in one call, passing the id that came back from its own <code>recall</code>:</p>
    ${code("json", `{
  "content": "Deploys run from the release branch, after the tag is signed.",
  "scope": "global",
  "supersedes": [12]
}`)}
    <p>Nothing is deleted. Memory 12 keeps its id, stays in <code>memory list</code> and the dashboards marked as replaced, still says what replaced it, and comes back at any time:</p>
    ${code("shell", `synapse memory show 12
synapse memory restore 12`)}
    <p>What changes is only what recall can see. A superseded memory stops being returned to tools and stops counting toward the number a session reports \u2014 the count a tool announces is the count it can actually draw on. Deleting the replacement restores what it replaced on its own, so a correction can never leave the older version hidden behind an id that no longer exists.</p>
    <p>Choose between the three: <strong>edit</strong> when the memory was badly worded and there is nothing to keep, <strong>supersede</strong> when the old version was true and stopped being true, and <strong>delete</strong> when it should never have been stored.</p>
    ${note("Supersession stays on this machine", "Sync carries a memory's content, not what replaced it. A memory superseded here arrives on another machine live. Until the wire format grows an operation for it, correct it on each machine you sync to.")}

    <h2 id="budgets">Response budgets</h2>
    <table>
      <thead><tr><th>Mode</th><th>Result limit</th><th>Character budget</th><th>Transformation</th></tr></thead>
      <tbody>
        <tr><td>Full</td><td>25</td><td>Unlimited</td><td>Returns stored formatting without compaction.</td></tr>
        <tr><td>Balanced</td><td>8</td><td>6,000</td><td>Compacts prose whitespace, preserves fenced and indented code, removes exact duplicate bodies, and replaces any memory too large for the remaining budget with its opening sentence.</td></tr>
        <tr><td>Lean</td><td>4</td><td>2,800</td><td>Uses the same non-destructive compaction with a smaller response.</td></tr>
      </tbody>
    </table>
    <p>The response reports the optimization mode actually applied. Choose the ceiling in <strong>Settings → Recall optimization</strong>, or use the CLI:</p>
    ${code("shell", `synapse settings show
synapse settings optimize lean
synapse settings optimize balanced`)}
    <p>A memory that will not fit in what is left of the budget is returned as its opening sentence, marked <code>abridged</code>, rather than cut off wherever the character count landed. Half a memory reads exactly like a whole one \u2014 <em>never deploy from main unless</em> is a rule with its condition amputated \u2014 and one long memory at the top of the results no longer costs you every result under it. A tool that receives an abridged memory is told to recall it again, more narrowly, before acting on the part it cannot see.</p>
    ${note("Originals remain intact", "Balanced and Lean transform only the MCP response. The database and memory-management views keep the complete stored body, abridged results included.")}

    <h2 id="destructive">Destructive actions</h2>
    <p>Deleting one memory, undoing an import, and wiping every memory require explicit confirmation. The desktop app presents a separate confirmation. A wipe removes memory and import history but leaves guidance, vault labels, Keychain values, scope approvals, and settings intact.</p>
    ${code("shell", `synapse memory delete 24 --confirm
synapse memory wipe --confirm`)}
    <p>Before a large cleanup, create a portable snapshot with <code>synapse data export</code>. Use a wipe when the goal is to clear context while retaining the vault setup; use a database restore when the goal is to return the entire Synapse state to an earlier snapshot.</p>
  `,
};

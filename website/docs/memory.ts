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

    <h2 id="control">Inspect and correct</h2>
    <p>The desktop <strong>Memories</strong> screen searches the same store and lets you inspect, edit, or delete individual entries. CLI edits replace the body and optionally the source:</p>
    ${code("shell", `printf '%s\n' 'Use Bun unless a task explicitly requires another runtime.' \\
  | synapse memory edit 24 tooling

synapse memory show 24`)}
    <p>Editing changes the stored original. Recall optimization does not. If a convention has become wrong, edit or delete it instead of adding a conflicting correction and hoping the tools choose the newer entry.</p>

    <h2 id="budgets">Response budgets</h2>
    <table>
      <thead><tr><th>Mode</th><th>Result limit</th><th>Character budget</th><th>Transformation</th></tr></thead>
      <tbody>
        <tr><td>Full</td><td>25</td><td>Unlimited</td><td>Returns stored formatting without compaction.</td></tr>
        <tr><td>Balanced</td><td>8</td><td>6,000</td><td>Compacts prose whitespace, preserves fenced and indented code, removes exact duplicate bodies, and truncates at a character boundary.</td></tr>
        <tr><td>Lean</td><td>4</td><td>2,800</td><td>Uses the same non-destructive compaction with a smaller response.</td></tr>
      </tbody>
    </table>
    <p>The response reports the optimization mode actually applied. Choose the ceiling in <strong>Settings → Recall optimization</strong>, or use the CLI:</p>
    ${code("shell", `synapse settings show
synapse settings optimize lean
synapse settings optimize balanced`)}
    ${note("Originals remain intact", "Balanced and Lean transform only the MCP response. The database and memory-management views keep the complete stored body.")}

    <h2 id="destructive">Destructive actions</h2>
    <p>Deleting one memory, undoing an import, and wiping every memory require explicit confirmation. The desktop app presents a separate confirmation. A wipe removes memory and import history but leaves guidance, vault labels, Keychain values, scope approvals, and settings intact.</p>
    ${code("shell", `synapse memory delete 24 --confirm
synapse memory wipe --confirm`)}
    <p>Before a large cleanup, create a portable snapshot with <code>synapse data export</code>. Use a wipe when the goal is to clear context while retaining the vault setup; use a database restore when the goal is to return the entire Synapse state to an earlier snapshot.</p>
  `,
};

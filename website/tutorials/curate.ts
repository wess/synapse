import { code, note } from "../markup";
import type { Page } from "../types";

export const curate: Page = {
  path: "tutorials/curate/index.html",
  title: "Curate and optimize memory",
  description: "Build a small memory set, search it, correct the durable source, compare recall budgets, and remove only the tutorial entries.",
  kind: "tutorial",
  toc: [
    { label: "Add entries", id: "add" },
    { label: "Search and inspect", id: "search" },
    { label: "Correct", id: "correct" },
    { label: "Compare budgets", id: "budgets" },
    { label: "Clean up", id: "cleanup" },
  ],
  body: `
    <h2>Outcome and prerequisites</h2>
    <p>You will create three harmless entries from stdin, use text and JSON search, correct one entry in place, compare Full and Lean response settings, then delete only the IDs created by this tutorial.</p>
    ${note("Record the returned IDs", "Every add command prints its new numeric ID. Save those three numbers for the cleanup step; do not use memory wipe in a store that contains real context.")}

    <ol class="steps">
      <li>
        <h3 id="add">Add focused entries</h3>
        ${code("shell", `printf '%s\n' 'Tutorial apps use port 4100.' | synapse memory add synapsetutorial
printf '%s\n' 'Tutorial commands use Bun.' | synapse memory add synapsetutorial
printf '%s\n' 'Tutorial backups go in the backups folder.' | synapse memory add synapsetutorial`)}
        <p>Write down the three IDs. Each entry contains one durable idea, which makes later recall easier to interpret and correct.</p>
      </li>
      <li>
        <h3 id="search">Search and inspect</h3>
        ${code("shell", `synapse memory list tutorial
synapse memory list "Tutorial Bun" --json
synapse memory show <bunid>`)}
        <p>Text output is optimized for scanning. JSON output includes exact bodies, sources, IDs, and timestamps for local scripts.</p>
      </li>
      <li>
        <h3 id="correct">Correct the source in place</h3>
        <p>Suppose the runtime convention becomes more precise. Replace the original instead of adding a contradiction:</p>
        ${code("shell", `printf '%s\n' 'Tutorial commands use Bun unless a task requires another runtime.' \\
  | synapse memory edit <bunid> synapsetutorial
synapse memory show <bunid>`)}
        <p>The ID remains stable and future recall returns the correction.</p>
      </li>
      <li>
        <h3 id="budgets">Compare response budgets</h3>
        ${code("shell", `synapse settings show
synapse settings optimize full`)}
        <p>Ask a connected tool to recall <code>synapsetutorial</code> and inspect the formatting and number of results. Then switch:</p>
        ${code("shell", `synapse settings optimize lean`)}
        <p>Recall the same query. Lean returns at most four compacted entries within 2,800 characters. The stored bodies visible through <code>memory show</code> remain unchanged.</p>
      </li>
      <li>
        <h3>Restore your preferred setting</h3>
        ${code("shell", `synapse settings optimize balanced`)}
        <p>Balanced is the default: up to eight results and 6,000 characters, with prose whitespace compaction and exact duplicate removal.</p>
      </li>
    </ol>

    <h2 id="cleanup">Delete only the tutorial entries</h2>
    ${code("shell", `synapse memory delete <portid> --confirm
synapse memory delete <bunid> --confirm
synapse memory delete <backupid> --confirm
synapse memory list synapsetutorial`)}
    <p>The final search should be empty unless another tutorial created the same source. Use the Memories screen for a visual review before deleting any additional record.</p>
  `,
};

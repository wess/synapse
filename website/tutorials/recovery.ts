import { code, note } from "../markup";
import type { Page } from "../types";

export const recovery: Page = {
  path: "tutorials/recovery/index.html",
  title: "Export and restore safely",
  description:
    "Create a validated snapshot, make one reversible change, stop every database user, restore, and prove the previous state came back — including what a snapshot deliberately does not carry.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "Check first", id: "check" },
    { label: "Export", id: "export" },
    { label: "Make a change", id: "change" },
    { label: "Stop every user", id: "stop" },
    { label: "Restore", id: "restore" },
    { label: "Verify", id: "verify" },
    { label: "What did not move", id: "notmoved" },
    { label: "Automatic backups", id: "automatic" },
    { label: "Moving to a new Mac", id: "newmac" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will take a consistent snapshot of the whole store, make a change you can see, restore over it, and prove the change is gone. Then you will understand exactly what a snapshot contains, what it deliberately leaves out, and how that affects moving to another machine.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed.</li>
      <li>A destination path that does not already exist.</li>
      <li>A terminal you can keep open, to record one memory ID.</li>
    </ul>
    ${note("This really does replace the active database", "The tutorial is designed to be reversible and Synapse writes a pre-restore snapshot of its own before it does anything. But restore is a genuine database replacement. Read each step before running it, and do not substitute a production backup you have not separately preserved.")}

    <ol class="steps">
      <li>
        <h3 id="check">Check the store before you trust a copy of it</h3>
        ${code("shell", `synapse path
synapse data check`)}
        ${code("text", `Database: /Users/example/Library/Application Support/synapse/brain.db
Version: 7
Integrity: ok`)}
        <p>Integrity must read <code>ok</code>. Two separate checks stand behind that word: a relationship check that runs every time the database opens, and a full page-by-page scan that reads the whole file. Exporting a store that fails either one copies the problem rather than saving you from it.</p>
        <p>If it reports anything else, stop here and follow the corruption guidance in <a href="../../docs/troubleshoot/">Troubleshooting</a> before continuing.</p>
      </li>

      <li>
        <h3 id="export">Create a fresh export</h3>
        ${code("shell", `backup="$HOME/Desktop/synapsetutorialbackup.db"
test ! -e "$backup"
synapse data export "$backup"`)}
        ${code("text", `Exported /Users/example/Desktop/synapsetutorialbackup.db`)}
        <p>That one line hides four steps. Synapse writes a consistent compact snapshot rather than copying a file that may be mid-write, secures its permissions to owner-only, reopens it read-only, and validates it — all before reporting success. An export that would not open is a failure here rather than a surprise months later.</p>
        <p>The snapshot is a complete SQLite database. It holds every memory with its scope and project, vault and secret <em>metadata</em>, scope trust records, import batches, skill install receipts, mesh history, and your settings.</p>
      </li>

      <li>
        <h3 id="change">Add one marker after the export</h3>
        ${code("shell", `printf '%s\\n' 'This entry exists only after the tutorial export.' \\
  | synapse memory add synapserecovery
synapse memory list synapserecovery`)}
        ${code("text", `Stored memory #42`)}
        <p>Record that ID. It is the marker that must disappear when the restore succeeds — which is what turns this from "the command did not error" into an actual proof.</p>
      </li>

      <li>
        <h3 id="stop">Stop every database user</h3>
        <p>Quit the Synapse desktop app. Close every connected Codex and Claude Code session so their <code>synapse mcp</code> child processes exit. A plain CLI command releases its lock when it finishes, so nothing needs doing about those.</p>
        <p>Reading the database takes a shared lock; restoring requires an exclusive one. If you deliberately leave a session open, the next step will refuse and tell you to close the app and connected tools.</p>
        ${note("Do not work around the lock", "The refusal is not a formality. Replacing a database file underneath a process that has it open is how a store gets corrupted in a way no integrity check can undo. If restore refuses, find the process that is still holding it.")}
      </li>

      <li>
        <h3 id="restore">Restore</h3>
        ${code("shell", `synapse data restore "$backup"`)}
        ${code("text", `Restored /Users/example/Desktop/synapsetutorialbackup.db
Previous database: /Users/example/Library/Application Support/synapse/backups/brain.1785776950101.restore.db`)}
        <p>Read the second line. Before replacing anything, Synapse wrote the <em>current</em> database into the backups folder with a <code>restore</code> marker in its name. If you have just restored the wrong file, that path is your way back — and it is printed rather than left for you to discover.</p>
        <p>The sequence is: validate the source, validate the current database, acquire the exclusive lock, snapshot the current state, then replace atomically. Any step failing leaves the original in place.</p>
      </li>

      <li>
        <h3 id="verify">Verify the earlier state actually returned</h3>
        ${code("shell", `synapse data check
synapse memory list synapserecovery`)}
        <p>Integrity should read <code>ok</code>, and the marker must be absent — an empty result is the success condition. Confirm something you expect to still be there as well, so you are testing that the right state returned rather than that the database is merely empty:</p>
        ${code("shell", `synapse memory list`)}
        <p>Reopen the desktop app and your connected tools only after this verification passes.</p>
      </li>
    </ol>

    <h2 id="notmoved">What a snapshot does not carry</h2>
    <p>This is the part that surprises people, and it follows directly from the security model.</p>
    <table>
      <thead><tr><th>Not in the export</th><th>Why, and what to do</th></tr></thead>
      <tbody>
        <tr><td>Secret values</td><td>They live in macOS Keychain and were never in the database. On the same Mac, restored metadata still points at the existing items and everything works. On another Mac, recreate each with <code>synapse secret set</code>.</td></tr>
        <tr><td>Your <code>SOUL.md</code></td><td>A file beside the database, not a table inside it. Copy it separately if it matters — and it usually does.</td></tr>
        <tr><td>Your skill library</td><td>Directories under the data folder. Copy <code>skills/</code> separately.</td></tr>
        <tr><td>Roles and teams</td><td>TOML under the data folder, plus anything in a project's own <code>.synapse/roles/</code>, which travels with that checkout instead.</td></tr>
        <tr><td>Tool configuration</td><td>Belongs to Codex and Claude Code. Reconnect on the new machine rather than copying their files.</td></tr>
      </tbody>
    </table>
    ${code("shell", `# a complete manual copy, when the database alone is not enough
cp -R ~/Library/Application\\ Support/synapse ~/Desktop/synapse-full-copy`)}

    <h2 id="automatic">Backups you did not ask for</h2>
    <p>Synapse writes snapshots on its own at two moments: before any schema migration, and before any restore. Both land in the same place:</p>
    ${code("shell", `ls ~/Library/Application\\ Support/synapse/backups/`)}
    <p>Only the newest few are kept, so the folder cannot grow without limit — the same rule that bounds worker logs and the crash log. That means an automatic backup is a safety net for the operation that just happened, not an archive. If you want a copy to keep, take an explicit export and move it somewhere Synapse does not manage.</p>

    <h2 id="newmac">Moving to a new Mac</h2>
    <ol class="steps">
      <li><h3>On the old machine</h3><p>Run <code>synapse data export</code>, and copy <code>SOUL.md</code> and the <code>skills/</code> directory alongside it. Note which secrets exist with <code>synapse secret list</code> for each vault — the names, since you cannot export the values.</p></li>
      <li><h3>On the new machine</h3><p>Install Synapse, install the CLI, then restore the export <em>before</em> connecting any tools. Put <code>SOUL.md</code> and <code>skills/</code> back in the data folder.</p></li>
      <li><h3>Then</h3><p>Recreate each secret with <code>synapse secret set</code>, re-approve each project scope with <code>synapse allow</code> — approval is per machine, because it is a statement about a file you have read — connect your tools, and run <code>synapse skill install</code>.</p></li>
    </ol>
    ${code("shell", `synapse doctor`)}
    <p>One command to confirm the new machine matches the old one: same schema version, same memory count, tools connected, skills installed.</p>

    <h2>Next step</h2>
    <p>Finish the maintainer level with <a href="../lifecycle/">Check, migrate, and remove Synapse</a>, which covers the health report in full and both removal paths.</p>
  `,
};

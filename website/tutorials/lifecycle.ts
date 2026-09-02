import { code, note } from "../markup";
import type { Page } from "../types";

export const lifecycle: Page = {
  path: "tutorials/lifecycle/index.html",
  title: "Check, migrate, and remove Synapse",
  description:
    "Read a full health report, understand what a schema migration does to your store, disconnect one tool, and remove everything Synapse installed while keeping your memory and your own files.",
  kind: "tutorial",
  toc: [
    { label: "Outcome", id: "outcome" },
    { label: "One health report", id: "doctor" },
    { label: "Check the store", id: "check" },
    { label: "Migrations", id: "migrations" },
    { label: "Disconnect one tool", id: "disconnect" },
    { label: "Read the uninstall preview", id: "preview" },
    { label: "Remove everything", id: "uninstall" },
    { label: "What survives", id: "survives" },
  ],
  body: `
    <h2 id="outcome">Outcome and prerequisites</h2>
    <p>You will produce the report to attach to a bug, verify the store's integrity and schema version, understand what happens when a release changes the schema, and then walk both removal paths — one tool, and everything — reading what each will do before it does it.</p>
    <ul>
      <li>The <code>synapse</code> CLI installed and at least one tool connected.</li>
      <li><a href="../recovery/">Export and restore safely</a> read first. This tutorial assumes you know how to take a snapshot; the removal steps are more comfortable when you have one.</li>
    </ul>
    ${note("Read before you confirm", "Every destructive command here refuses to act without <code>--confirm</code> and prints what it would do instead. That preview is the feature. Run each command once without the flag and actually read the output before adding it.")}

    <ol class="steps">
      <li>
        <h3 id="doctor">Get the whole picture in one command</h3>
        ${code("shell", `synapse doctor`)}
        ${code("text", `Synapse 0.1.0-beta.24

Store
  State          ok
  Schema         v10
  Memories       0
  Size           0 MB
  Backups        0
  Recall budget  balanced

Connected tools
  Claude Code    installed, not connected
    version      2.1.241 (Claude Code)
    guidance     no
    notice       no
    compaction   no
    status line  none
  Codex          installed, not connected
    version      codex-cli 0.147.0
    guidance     no
    notice       no
    compaction   no
    status line  none
  pi             not installed

Skills
  In the library 1
  Installed      0
  Out of date    0
  Agents write   no`)}
        <p>This is what to attach to a bug report. It answers most first questions without a round trip: which version, whether the store is sound, what schema it is on, whether each tool is actually connected rather than merely installed, and whether the guidance pointer, session notice, and status line are in place.</p>
        <p><code>--json</code> gives the same thing structured. Note what is <em>not</em> here: no memory contents, no secret names, no file paths outside Synapse's own. A doctor report is safe to paste into a public issue.</p>
        <p>Read the tool rows carefully. <code>installed, not connected</code> means the tool is on the machine but has no Synapse entry. <code>Agents write</code> under Skills is whether self-improvement is on, and a count of anything waiting for you appears under it when there is any. A tool that was connected and whose executable later moved reports as stale rather than healthy, because detection resolves the stored command rather than trusting the entry exists.</p>
      </li>

      <li>
        <h3 id="check">Check the store itself</h3>
        ${code("shell", `synapse data check
synapse data check --json`)}
        ${code("text", `{
  "path": "~/Library/Application Support/synapse/brain.db",
  "version": 7,
  "integrity": "ok"
}`)}
        <p>Two different checks run when the database opens, with very different costs. A relationship check is index-driven and runs every single time. A full page-by-page scan reads the whole file, so it runs once per process against an unchanged file — which is why a status line redrawing on every turn stays cheap while <code>data check</code> is thorough.</p>
        <p><code>integrity: ok</code> means both passed. Anything else is reported rather than worked around, and <a href="../recovery/">restoring a snapshot</a> is the answer.</p>
      </li>

      <li>
        <h3 id="migrations">Understand what a migration does</h3>
        <p><code>Schema v7</code> in the report above is the store's version. Each release that changes the schema adds a numbered migration and raises that number. Opening the database applies any that are missing, in order.</p>
        <p>Three things are true of every migration and worth knowing before you upgrade:</p>
        <ul>
          <li><strong>A backup is taken first.</strong> Before any migration runs against an existing store, Synapse copies it into the backups folder. If a migration fails, that copy is what you restore.</li>
          <li><strong>Shipped migrations are never edited.</strong> A release only ever appends. That is what makes the upgrade path from any older version deterministic rather than dependent on which releases you happened to install.</li>
          <li><strong>A newer store refuses an older binary.</strong> If the database reports a version this release does not know about, Synapse says so and stops rather than guessing at a schema from the future.</li>
        </ul>
        ${code("shell", `ls ~/Library/Application\\ Support/synapse/backups/`)}
        <p>Backups are bounded — only the newest few are kept — so the folder cannot grow without limit. The same rule applies everywhere Synapse writes something unbounded: worker logs and the crash log keep their tail rather than the whole history.</p>
        ${note("Upgrading is just opening it", "There is no migrate command and nothing to run by hand. Install a new release, run any <code>synapse</code> command or open the app, and the store is brought forward with a backup beside it. <code>synapse doctor</code> afterwards will show the new schema number.")}
      </li>

      <li>
        <h3 id="disconnect">Disconnect one tool</h3>
        <p>Disconnecting undoes one tool's integration and nothing else:</p>
        ${code("shell", `synapse disconnect claude`)}
        ${code("text", `Removed Claude Code skill \`synapse-mesh\`
warning: Claude Code skill \`release-checklist\`: \`release-checklist\` in Claude Code has been changed since Synapse installed it`)}
        <p>Read that second line closely, because it is the whole design in one sentence. Synapse removed the skill it had written and left untouched. It refused to remove the one that had been edited since — because at that point the file is partly yours, and Synapse will not delete work it cannot prove is its own.</p>
        <p>The same run also asks the tool's own CLI to forget the MCP server, strips the managed block from its instruction file while leaving your text in place, and removes the session hook and status line from its settings — but only if they are the ones Synapse wrote. A status line you configured yourself is reported and left alone.</p>
        <p>Every step reports rather than aborts. A disconnect that stopped at the first problem would leave the tool half-connected, which is the worst available outcome; finishing and telling you what it could not do is better.</p>
        ${code("shell", `synapse disconnect`)}
        <p>With no tool named, it disconnects every one.</p>
      </li>

      <li>
        <h3 id="preview">Read the uninstall preview</h3>
        <p>Run it without <code>--confirm</code> first. It always previews:</p>
        ${code("shell", `synapse uninstall`)}
        ${code("text", `\`synapse uninstall --confirm\` would remove:
  · every skill Synapse installed, leaving any you wrote

Your memory in ~/Library/Application Support/synapse would be left alone.

Add --confirm to go ahead.`)}
        <p>On a fully connected machine the list is longer — the MCP entries, the managed instruction blocks, the session hook, the status line, the installed CLI, the shell startup block. The last line is the one to notice: <strong>memory is never removed as a side effect.</strong> Uninstalling the software does not delete what you have taught it.</p>
      </li>

      <li>
        <h3 id="uninstall">Remove everything, including memory</h3>
        <p>Taking the data folder requires asking for it by name, and the preview changes to say so:</p>
        ${code("shell", `synapse uninstall --data`)}
        ${code("text", `\`synapse uninstall --confirm\` would remove:
  · every skill Synapse installed, leaving any you wrote

And, because --data was given, everything in ~/Library/Application Support/synapse
including all of your memory. That cannot be undone.

Add --confirm to go ahead.`)}
        <p>Two flags, both required, and a sentence that says it cannot be undone. If you want a copy first, <code>synapse data export</code> writes a consistent snapshot you can restore into a fresh install later — see <a href="../recovery/">Export and restore safely</a>.</p>
        ${code("shell", `synapse data export ~/synapse-final-snapshot.db   # optional, but do it
synapse uninstall --data --confirm`)}
        ${note("Check where your values live first", "On the encrypted store, <code>vault.db</code> and <code>vault.key</code> are in the data folder, so <code>--data</code> deletes every secret value with it. The command says so before it acts. On the Keychain store the values are outside the folder and survive: removing it costs Synapse the ability to find them, so use <code>synapse secret forget</code> for those first. <code>synapse vault backend</code> says which store this machine uses.")}
      </li>
    </ol>

    <h2 id="survives">What survives</h2>
    <p>The principle behind all of this: <strong>everything Synapse writes outside its own folder, it can take back — and nothing else.</strong> After a full uninstall, these are untouched:</p>
    <table>
      <thead><tr><th>Thing</th><th>Why it survives</th></tr></thead>
      <tbody>
        <tr><td>Your own words in <code>CLAUDE.md</code> or <code>AGENTS.md</code></td><td>Only the managed block between markers is removed. Text outside it was never Synapse's.</td></tr>
        <tr><td>A skill you wrote or edited</td><td>Removal requires an install record proving Synapse wrote that exact content.</td></tr>
        <tr><td>A status line somebody else configured</td><td>JSON has no comments, so entries are recognized by the command they run. One that is not Synapse's is reported, never replaced.</td></tr>
        <tr><td>Other MCP servers in either tool</td><td>Removal goes through the tool's own CLI, by name, for the <code>synapse</code> entry only.</td></tr>
        <tr><td>Your memory, unless <code>--data</code></td><td>Memory is never removed as a side effect of removing software.</td></tr>
        <tr><td>Secret values on the Keychain store</td><td>They were never in the data folder, so removing it cannot delete them. On the encrypted store they are <em>in</em> that folder, and <code>--data</code> takes them with it.</td></tr>
        <tr><td>A project's <code>.synapse.yaml</code> and <code>.synapse/roles/</code></td><td>They belong to the checkout, not to your machine.</td></tr>
      </tbody>
    </table>

    <h2>Next step</h2>
    <p>That is the whole maintainer track. If something in it did not behave as described, <code>synapse doctor --json</code> plus the exact command you ran is everything a bug report needs. See <a href="../../docs/troubleshoot/">Troubleshooting</a> for the common failures and what each one means.</p>
  `,
};

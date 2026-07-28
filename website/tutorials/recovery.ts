import { code, note } from "../markup";
import type { Page } from "../types";

export const recovery: Page = {
  path: "tutorials/recovery/index.html",
  title: "Export and restore safely",
  description: "Create a validated snapshot, make one reversible memory change, stop all database users, restore, and prove the previous state returned.",
  kind: "tutorial",
  toc: [
    { label: "Check and export", id: "export" },
    { label: "Make a change", id: "change" },
    { label: "Stop database users", id: "stop" },
    { label: "Restore", id: "restore" },
    { label: "Verify", id: "verify" },
  ],
  body: `
    <h2>Outcome and prerequisites</h2>
    <p>You will prove a full SQLite snapshot can restore prior memory state without handling secret values. Choose a new destination path that does not exist. Keep the terminal open so you can record the test memory ID.</p>
    ${note("This changes the active database", "The tutorial is designed to be reversible and Synaps creates a pre-restore backup, but restore is still a real database replacement. Read every step and do not substitute a production backup you have not separately preserved.")}

    <ol class="steps">
      <li>
        <h3 id="export">Check the active database</h3>
        ${code("shell", `synaps path
synaps data check`)}
        <p>Integrity must report <code>ok</code>. If it does not, stop here and follow the corruption guidance in Troubleshooting.</p>
      </li>
      <li>
        <h3>Create a fresh export</h3>
        ${code("shell", `backup="$HOME/Desktop/synapstutorialbackup.db"
test ! -e "$backup"
synaps data export "$backup"`)}
        <p>Synaps writes a consistent compact snapshot, secures it, opens it read-only, and validates it before returning success.</p>
      </li>
      <li>
        <h3 id="change">Add one post-export memory</h3>
        ${code("shell", `printf '%s\n' 'This entry exists only after the tutorial export.' \\
  | synaps memory add synapsrecovery
synaps memory list synapsrecovery`)}
        <p>Record the returned ID. This entry is the marker that should disappear after restoration.</p>
      </li>
      <li>
        <h3 id="stop">Stop every database user</h3>
        <p>Quit the Synaps desktop app. Close all connected Codex and Claude Code sessions so their <code>synaps mcp</code> child processes exit. A normal CLI command releases its shared lock when it finishes.</p>
        <p>If you intentionally leave a connected session open, the next step should refuse with a message telling you to close the app and connected tools. Do not work around the lock.</p>
      </li>
      <li>
        <h3 id="restore">Restore the export</h3>
        ${code("shell", `synaps data restore "$backup"`)}
        <p>The command validates the source and current database, acquires the exclusive lock, creates an automatic <code>restore</code> recovery snapshot of the current state, then atomically replaces the database.</p>
      </li>
      <li>
        <h3 id="verify">Verify the earlier state</h3>
        ${code("shell", `synaps data check
synaps memory list synapsrecovery`)}
        <p>Integrity should be <code>ok</code>, and the post-export marker should be absent. Reopen the desktop app and connected tools only after this verification.</p>
      </li>
    </ol>

    <h2>Understand what did not move</h2>
    <p>Keychain values are not inside the export. On the same Mac, restored vault metadata still points at the existing items. On another Mac, recreate each expected value with <code>synaps secret set</code>.</p>
    <p>Keep the export if it is part of your backup plan. Otherwise, move it to Trash after you are satisfied with the restore. The automatic pre-restore snapshot remains in the Synaps <code>backups</code> directory until you manage it deliberately.</p>
  `,
};

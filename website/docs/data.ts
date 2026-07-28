import { code, note } from "../markup";
import type { Page } from "../types";

export const data: Page = {
  path: "docs/data/index.html",
  title: "Data lifecycle",
  description: "Locate the database, understand startup checks and automatic backups, export a consistent snapshot, and restore safely.",
  kind: "docs",
  toc: [
    { label: "Files and permissions", id: "files" },
    { label: "Startup", id: "startup" },
    { label: "Automatic backups", id: "backups" },
    { label: "Export", id: "export" },
    { label: "Restore", id: "restore" },
    { label: "Memory wipe", id: "wipe" },
  ],
  body: `
    <h2 id="files">Files and permissions</h2>
    <p>On macOS, the default data directory is <code>~/Library/Application Support/synaps</code>. Run <code>synaps path</code> to print the resolved home, data, database, and CLI paths for the current environment.</p>
    <table>
      <thead><tr><th>Path</th><th>Purpose</th></tr></thead>
      <tbody>
        <tr><td><code>brain.db</code></td><td>Memory, settings, vault metadata, global mappings, and scope approvals.</td></tr>
        <tr><td><code>brain.db-wal</code> and <code>brain.db-shm</code></td><td>SQLite write-ahead-log sidecars while the database is active.</td></tr>
        <tr><td><code>brain.lock</code></td><td>Shared lifecycle lock held by the app, CLI operations, and MCP server.</td></tr>
        <tr><td><code>backups/</code></td><td>Automatic pre-migration and pre-restore SQLite snapshots.</td></tr>
      </tbody>
    </table>
    <p>On Unix systems the data and backup directories use mode <code>0700</code>; database files, sidecars, locks, and backups use <code>0600</code>.</p>

    <h2 id="startup">Startup</h2>
    <p>Every database open follows the same sequence:</p>
    <ol>
      <li>Create and secure the data directory if needed.</li>
      <li>Acquire a shared lifecycle lock.</li>
      <li>Open SQLite with foreign keys enabled, a five-second busy timeout, and WAL journal mode.</li>
      <li>Run <code>PRAGMA quick_check</code> and <code>PRAGMA foreign_key_check</code>.</li>
      <li>Reject a database newer than the current application supports.</li>
      <li>Create a pre-migration snapshot when an existing database needs a numbered migration, then apply the migration transactionally.</li>
      <li>Reapply owner-only file permissions.</li>
    </ol>
    ${code("shell", `synaps data check
synaps data check --json`)}

    <h2 id="backups">Automatic backups</h2>
    <p>Before changing an existing schema, Synaps creates <code>backups/brain.&lt;timestamp&gt;.v&lt;version&gt;.db</code>. Before replacing an existing database during restore, it creates <code>backups/brain.&lt;timestamp&gt;.restore.db</code>.</p>
    <p>These are complete snapshots created through SQLite <code>VACUUM INTO</code>, not copies of a live database file. Synaps does not currently rotate them; include the folder in your normal local backup policy and remove old snapshots deliberately.</p>

    <h2 id="export">Export a portable snapshot</h2>
    ${code("shell", `synaps data export "$HOME/Desktop/synapsbackup.db"
synaps data check --json`)}
    <p>The destination must not already exist. Synaps opens the active database normally, writes a consistent compact snapshot, secures its permissions, then reopens it read-only and validates integrity before reporting success.</p>
    ${note("Keychain values are separate", "The exported database contains vault metadata and Keychain account references, but not secret values. Restoring it on another Mac does not transfer those values.")}

    <h2 id="restore">Restore a snapshot</h2>
    <ol>
      <li>Quit the Synaps desktop app.</li>
      <li>Stop every connected MCP process by closing the relevant tool sessions.</li>
      <li>Run the restore command from a separate terminal.</li>
    </ol>
    ${code("shell", `synaps data restore "$HOME/Desktop/synapsbackup.db"
synaps data check`)}
    <p>Restore opens the source read-only, validates page and relationship integrity, and requires its schema version to match the current release. It then tries to acquire the exclusive lifecycle lock. If the app or any MCP server still holds the database, restore refuses without changing anything.</p>
    <p>When a current database exists, Synaps validates it and creates a recovery snapshot before atomically replacing it. WAL and SHM sidecars are cleared, file permissions are secured, and the containing directory is synced.</p>

    <h2 id="wipe">Memory wipe is not a reset</h2>
    <p><code>synaps memory wipe --confirm</code> removes every memory entry but leaves settings, vault labels, Keychain values, global mappings, and scope approvals. Use it when you want a clean memory history without rebuilding credential setup.</p>
    <p>There is no one-command factory reset. To remove Synaps completely, first forget each Keychain secret through the app or CLI, then quit all processes and remove the application, installed launcher, and data directory deliberately.</p>
  `,
};

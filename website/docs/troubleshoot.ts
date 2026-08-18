import { code, note } from "../markup";
import type { Page } from "../types";

export const troubleshoot: Page = {
  path: "docs/troubleshoot/index.html",
  title: "Troubleshooting",
  description: "Diagnose installation, tool connection, scope, Keychain, memory, database, and restore failures without risking user-owned data.",
  kind: "docs",
  toc: [
    { label: "Start with status", id: "status" },
    { label: "One command for a bug report", id: "doctor" },
    { label: "App and CLI", id: "app" },
    { label: "Tool connection", id: "connection" },
    { label: "Scopes and secrets", id: "scopes" },
    { label: "Memory and database", id: "database" },
    { label: "Restore", id: "restore" },
  ],
  body: `
    <h2 id="status">Start with status</h2>
    ${code("shell", `synapse version
synapse path
synapse data check
synapse status .
synapse settings show`)}
    <p>These commands establish which executable is running, which database it opens, whether the database is healthy, which scopes apply to the current folder, and which response budget connected tools use.</p>

    <h2 id="doctor">One command for a bug report</h2>
    <p><code>synapse doctor</code> gathers everything anyone would ask you for: version, store state and size, which tools are connected and what each is set up with, skill and mesh state, shell and CLI integration, resolved paths, and any crash Synapse has recorded.</p>
    ${code("shell", `synapse doctor
synapse doctor --json`)}
    <p>Every check reports rather than fails, so a damaged store is described in the report instead of ending it. Synapse sends nothing anywhere &mdash; the report is printed for you to read or paste.</p>
    ${note("The desktop application writes panics to <code>crash.log</code> in the data folder, because a window that disappears leaves nothing to read. The log holds only the crash itself and is bounded, so a crash loop cannot fill a disk. <code>doctor</code> prints the most recent entries.")}

    <h2 id="app">App and CLI</h2>
    <h3>macOS will not open the app</h3>
    <p>Confirm you downloaded the current archive from the release page, extracted it fully, and moved the app to Applications. The app is Developer ID signed and notarized, so a Gatekeeper refusal usually means the archive was altered or only partly extracted. Avoid modifying files inside the application bundle because that invalidates its signature.</p>
    <h3><code>synapse: command not found</code></h3>
    <p>Install the CLI from Settings, then add <code>~/.local/bin</code> to <code>PATH</code>. Run the executable by full path once to verify:</p>
    ${code("shell", `~/.local/bin/synapse version`)}
    <h3>The CLI installer reports a conflict</h3>
    <p>A different file already exists at the destination and does not have a matching Synapse receipt. Synapse will not replace it. Move or rename that file yourself, or set <code>SYNAPSE_BIN</code> to a different full destination and install again.</p>
    <h3>The CLI stopped working after moving the app</h3>
    <p>Packaged installations point into the signed bundle. Open Synapse at its final location and choose <strong>Install CLI</strong> again to refresh the managed launcher.</p>

    <h2 id="connection">Tool connection</h2>
    <h3>A tool is detected but not connected</h3>
    <p>The registered entry may point to a deleted build or old app location, or may have arguments other than <code>["mcp"]</code>. Choose <strong>Repair</strong> in the app. Synapse backs up the integration store before replacing the named entry.</p>
    <h3>The tools do not appear after connection</h3>
    <p>Restart the developer tool so it relaunches user-level MCP servers. Confirm the stored command exists and runs <code>synapse mcp</code> without printing shell startup text to stdout.</p>
    <h3>The tools follow different global guidance</h3>
    <p>Run <code>synapse guidance show</code>. If a pointer is missing, use <code>synapse guidance sync</code> or <strong>Settings → Shared guidance → Sync pointers</strong>, then restart the tools. Consolidation is optional and requires separate confirmation.</p>
    <h3>Setup failed</h3>
    <p>Read the app error for the external tool command that failed. Setup restores both the integration store and instruction file on failure, leaving <code>.synapsebackup</code> recovery copies beside changed existing files.</p>

    <h2 id="scopes">Scopes and secrets</h2>
    <h3><code>vault scope is not ready</code></h3>
    <p>Run <code>synapse status .</code>. Approve pending or changed files with <code>synapse allow</code> only after inspecting them. Fix invalid YAML and unknown <code>vault.name</code> references before trying either shell mode again.</p>
    <h3>The shell hook does not activate</h3>
    <p>Open Settings and check <strong>Shell environments</strong>. Enable or repair the detected hook, then open a new terminal. Run <code>synapse status .</code>; it should report the current shell hook and ambient state as ready. Approve the closest scope with <code>synapse allow</code>. Global mappings alone never activate ambient mode.</p>
    <h3>Settings reports an incomplete or duplicate hook block</h3>
    <p>Synapse refuses to guess which startup-file content it owns. Open the path shown in Settings, reduce the <code># synapse:shell:begin</code> and <code># synapse:shell:end</code> markers to one complete pair or remove that marked block, then return to Settings and enable the hook again.</p>
    <h3>An ambient value remains after leaving</h3>
    <p>Press Enter once so the prompt hook reevaluates the directory. If the name existed before activation, Synapse deliberately restores that original value. Run <code>synapse status .</code> to distinguish a restored value from an active scope.</p>
    <h3>A narrower mapping does not apply</h3>
    <p>Check every ancestor scope. A broader <code>deny</code> permanently blocks that environment name for narrower scopes. Also confirm the nested file itself is approved and the command’s working directory is below it.</p>
    <h3>Keychain access fails</h3>
    <p>Unlock the login Keychain and retry from an interactive user session. If a label exists but its item was removed externally, set the secret again. If the credential itself may be invalid, rotate it at its issuer before storing the replacement.</p>
    <h3>The child sees a value but MCP does not</h3>
    <p>This is expected. MCP reports only names and trust state. Values are read from Keychain only for <code>synapse run -- &lt;command&gt;</code> or while an installed shell hook activates an approved directory.</p>

    <h2 id="database">Memory and database</h2>
    <h3>Recall returns too little content</h3>
    <p>Run <code>synapse settings show</code>. Switch to Balanced or Full if Lean is too small. Search with concrete words present in the stored body and inspect the exact entry through <code>synapse memory show &lt;id&gt;</code>.</p>
    <h3>A memory that is definitely stored does not come back</h3>
    <p>Make the search show its working with <code>synapse memory list "your query" --explain</code>. It prints the words it searched for and the words it dropped for matching nearly every memory, which separates a query that lost its only real term from a store that holds nothing. If what you are looking for is an exact string — a flag, an identifier, a path — use <code>synapse memory grep</code> instead, which matches characters and never drops a word. Also check the entry has not been superseded: <code>synapse memory list</code> marks a replaced memory, <code>synapse memory show &lt;id&gt;</code> names what replaced it, and <code>synapse memory restore &lt;id&gt;</code> puts it back in recall.</p>
    <h3>A stored memory is wrong</h3>
    <p>If the wording was bad, edit the original. If it was true and stopped being true, add the new version and run <code>synapse memory supersede &lt;old&gt; &lt;new&gt;</code> — recall returns the new one and the old text stays readable. If it should never have been stored, delete it. What to avoid is adding a contradictory entry and leaving both live, because recall returns both. Export a snapshot before a large cleanup.</p>
    <h3>An import shows flagged entries</h3>
    <p>Open the provider folder with <strong>Review source</strong> and inspect the named files. The app never imports flagged content. If a CLI import is genuinely safe, rerun it with both <code>--include-flagged</code> and <code>--confirm</code>; otherwise move only the durable non-sensitive fact into Synapse manually.</p>
    <h3>One project's memory appears missing</h3>
    <p>Inspect the record's project root in the Memory editor and compare it with the current repository root. Project recall intentionally combines global memory with one matching project and excludes every other project.</p>
    <h3>Integrity check fails</h3>
    <p>Stop using the affected database. Do not overwrite it. Preserve the whole data directory, locate the newest known-good export or automatic backup, and follow the restore procedure. A failed check is a data-recovery event, not a migration prompt.</p>

    <h2 id="restore">Restore</h2>
    <h3>Restore says Synapse is using the database</h3>
    <p>Quit the desktop app and close every developer-tool session connected to the MCP server. Confirm no <code>synapse mcp</code> process remains, then retry. The refusal proves the exclusive lifecycle guard is working.</p>
    <h3>The backup version is unsupported</h3>
    <p>The source schema must match the current release. Keep the source unchanged. Open it first with the Synapse version that created it, let that version migrate normally if appropriate, export a fresh snapshot, then restore that compatible export with the current app.</p>
    <h3>Secrets are missing after moving to another Mac</h3>
    <p>Database exports do not contain Keychain values. Recreate each value with <code>synapse secret set</code> on the destination Mac. Existing vault labels and references in the restored database tell you which entries are expected.</p>
  `,
};

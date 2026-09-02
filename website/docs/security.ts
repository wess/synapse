import { note } from "../markup";
import type { Page } from "../types";

export const security: Page = {
  path: "docs/security/index.html",
  title: "Security model",
  description: "Understand the trust boundary around local memory, stored secret values, MCP metadata, approved scopes, child processes, and configuration writes.",
  kind: "docs",
  toc: [
    { label: "Trust boundary", id: "boundary" },
    { label: "Memory", id: "memory" },
    { label: "Secret values", id: "secrets" },
    { label: "Scope approval", id: "scopes" },
    { label: "Environment boundaries", id: "children" },
    { label: "Operational limits", id: "limits" },
  ],
  body: `
    <h2 id="boundary">Trust boundary</h2>
    <p>Synapse is local-first, not a sandbox. It removes unnecessary network and file exposure from the memory-and-credential workflow, but it still runs as your macOS user. Any process already able to control your account, inspect your terminal, read your home directory, or access an unlocked Keychain may sit inside the same trust boundary.</p>
    <p>The application requires no Synapse account or hosted service. MCP uses local stdio. Memory and metadata live in a local SQLite file. Synapse includes no telemetry, no remote synchronization, and no web server.</p>

    <h2 id="memory">Memory</h2>
    <p>Memory bodies are plain text in SQLite and are available to connected MCP clients through recall. Global records are available in every project; project records are returned only with the matching normalized project root. Do not store credentials, private keys, access tokens, or material you would not want an authorized connected tool to read.</p>
    <p>Import reads recognized durable-memory stores only. It does not inspect conversation logs, authentication files, settings, tasks, or global instructions. Credential-shaped entries are flagged, hidden in previews, and skipped by the app. Provider formats are schema-checked before use, original files remain untouched, and import batches can be undone.</p>
    <p>Database files use owner-only permissions on Unix systems and are checked for page and foreign-key integrity before use. These controls protect against accidental broad access and corruption; they do not encrypt memory at rest.</p>

    <h2 id="secrets">Secret values</h2>
    <p>A secret value is never in <code>brain.db</code>, never in YAML, never in a log, never in an MCP response, and never at rest in plaintext. <code>brain.db</code> stores the vault, label, environment name, and account reference. The value goes to one of two stores.</p>
    <p><strong>The encrypted store</strong> is the default, and the only one available off macOS. Each value is sealed with XChaCha20-Poly1305 under a random 24-byte nonce, with its account reference authenticated as associated data, and written to <code>vault.db</code> in the data folder. The 32-byte key lives beside it in <code>vault.key</code>, created once with owner-only permissions and never overwritten. Deleted rows are zeroed rather than left on a free page.</p>
    <p>Be clear about what that protects. It protects a vault that has been <em>copied</em> — a backup, a synced folder, a disk image, a laptop somebody else now has. It does not protect you from a process already running as your user, which can read <code>vault.key</code> exactly the way Synapse reads it. This is encryption at rest, not an access control.</p>
    <p><strong>macOS Keychain</strong> is the other store, and on a Mac it is the stronger of the two: it adds per-application access control that the operating system enforces, which a key file cannot. It is one setting away in either direction, and <code>synapse vault migrate</code> moves every value across without a gap.</p>
    <p><code>secret set</code> does not accept a value argument. Interactive input uses a hidden prompt; piped input remains available for automation. <code>secret copy</code> is the only way a stored value comes back out, and it goes to the clipboard rather than to a screen, handed to <code>pbcopy</code> over stdin rather than as an argument that would be readable in <code>ps</code>. Anything on the clipboard is readable by every program that can ask for it, and Synapse does not clear it for you.</p>
    ${note("Metadata can still be sensitive", "Names such as PRODUCTION_DATABASE_URL can reveal system structure even when the value is hidden. Choose scope files and variable names with the same care you apply to ordinary repository configuration.")}

    <h2 id="scopes">Scope approval</h2>
    <p>An unapproved, edited, or invalid <code>.synapse.yaml</code> never contributes variables. Approval stores a SHA-256 digest of the exact file bytes and its canonical path. Any later edit invalidates that digest.</p>
    <p>Resolution walks from broad ancestor folders toward the working folder. Narrower approved files can replace a mapping, but a name denied by a broader scope remains denied. Any scope warning prevents <code>synapse run</code> from launching and makes the shell hook unload its managed values, avoiding silent partial configuration.</p>

    <h2 id="children">Environment boundaries</h2>
    <p><code>synapse run -- &lt;command&gt;</code> reads only the selected values out of the vault and places them in the environment of a new child process. That command and all of its descendants can read those values normally. Synapse cannot stop the child from logging, transmitting, or persisting them.</p>
    <p>The optional zsh, bash, or fish hook asks the shell to evaluate quoted environment changes. It activates only inside a directory with at least one approved discovered scope. Leaving, revoking approval, or changing any discovered scope unloads managed variables and restores values that existed before activation.</p>
    <p>Settings installs the hook inside a marked startup-file block using the absolute managed CLI path. It backs up and atomically replaces an existing file, refuses incomplete or duplicate Synapse blocks, and removes only the marked block when disabled.</p>
    ${note("Ambient access is broader", "Every process launched from an activated shell inherits the values, including commands that do not need them. Use command-scoped mode for a smaller boundary, and do not enable shell tracing while environment changes are evaluated.")}
    <ul>
      <li>Inspect scripts before launching them with production credentials.</li>
      <li>Use folder scopes and <code>deny</code> to narrow access.</li>
      <li>Prefer separate least-privilege credentials for development and deployment.</li>
      <li>Use <code>synapse status .</code> immediately before a sensitive command.</li>
      <li>Use <code>synapse deny</code> when a directory should no longer activate automatically.</li>
    </ul>

    <h2 id="limits">Operational limits</h2>
    <ul>
      <li>Synapse does not isolate a malicious connected MCP client from recalled memory. Only connect tools you trust.</li>
      <li>Both stores protect a value at rest, but an authorized local child receives plaintext environment variables.</li>
      <li>On the encrypted store, the key sits beside the sealed values in the data folder. Anything running as you can open the vault; a copy of the folder taken without the key cannot.</li>
      <li>Database exports contain <code>brain.db</code> only — memory and credential metadata, never a value. <code>vault.db</code> is a separate file for exactly that reason, and it is not in the export.</li>
      <li>Deleting the data folder deletes <code>vault.db</code> and <code>vault.key</code> with it, which is every stored value. On the Keychain store the values survive it. <code>synapse uninstall</code> says so before it does anything.</li>
      <li><code>SOUL.md</code> is ordinary local Markdown. Both connected tools can read it, so do not put secrets there.</li>
      <li>Configuration writes are validated, backed up, and atomic, but external programs can still change the same files afterward.</li>
      <li>A scope digest proves that you approved exact bytes at a path; it does not prove the commands launched from that folder are safe.</li>
      <li>There is no remote revocation system. Forget a secret in Synapse and rotate it at its issuer when exposure is possible.</li>
    </ul>
  `,
};

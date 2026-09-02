import { code, note } from "../markup";
import type { Page } from "../types";

export const vault: Page = {
  path: "docs/vault/index.html",
  title: "Vaults and scopes",
  description: "Keep values in an encrypted store or macOS Keychain, map names through approved YAML, understand precedence, and choose a command-scoped or ambient environment.",
  kind: "docs",
  toc: [
    { label: "Storage model", id: "model" },
    { label: "Where values live", id: "backend" },
    { label: "Create a secret", id: "create" },
    { label: "Get a value back", id: "copy" },
    { label: "Scope files", id: "files" },
    { label: "Approval", id: "approval" },
    { label: "Precedence and deny", id: "precedence" },
    { label: "Choose a boundary", id: "modes" },
  ],
  body: `
    <h2 id="model">Storage model</h2>
    <p>A vault is an organizational name. A secret record connects a vault-local label to an environment-variable name and an account reference. <code>brain.db</code> stores that metadata and never a value; the value itself goes to whichever store this machine keeps values in.</p>
    <p>A reference uses <code>vault.name</code> form, such as <code>work.database</code>. Scope YAML maps an environment name such as <code>DATABASE_URL</code> to that reference. A repository can therefore contain the mapping without containing the value.</p>
    <table>
      <thead><tr><th>Object</th><th>Example</th><th>Rules</th></tr></thead>
      <tbody>
        <tr><td>Vault</td><td><code>work</code></td><td>Letters, numbers, and hyphens; unique name.</td></tr>
        <tr><td>Secret name</td><td><code>database</code></td><td>Letters, numbers, and hyphens; unique inside its vault.</td></tr>
        <tr><td>Environment name</td><td><code>DATABASE_URL</code></td><td>Starts with a letter or underscore; then letters, numbers, or underscores; stored uppercase.</td></tr>
        <tr><td>Reference</td><td><code>work.database</code></td><td>Vault and secret name joined by a dot.</td></tr>
      </tbody>
    </table>

    <h2 id="backend">Where values live</h2>
    <p>Synapse has two value stores, and the choice is yours rather than the platform's.</p>
    <table>
      <thead><tr><th>Store</th><th>What it is</th><th>What it protects</th></tr></thead>
      <tbody>
        <tr><td><code>encrypted</code></td><td><code>vault.db</code> in the data folder, one XChaCha20-Poly1305 envelope per secret, sealed with a 32-byte key in <code>vault.key</code>. Both files are owner-only.</td><td>A vault that has been copied — a backup, a synced folder, a disk image, a machine somebody else now has.</td></tr>
        <tr><td><code>keychain</code></td><td>macOS Keychain, one generic password per secret under the <code>app.synapse.vault</code> service.</td><td>The same, plus per-application access control enforced by macOS.</td></tr>
      </tbody>
    </table>
    <p>The encrypted store is the default on a new installation and the only one available off macOS. A machine that was already holding secrets before this release stays on Keychain until you move it, so an upgrade never stops resolving a credential.</p>
    ${code("shell", `synapse vault backend
synapse vault migrate keychain
synapse vault migrate encrypted --keep`)}
    <p><code>vault backend</code> with no argument prints the current store. With an argument it only records a choice, and refuses once secrets exist — moving is <code>vault migrate</code>, which copies every value, reads each one back, switches the setting, and then removes the originals. <code>--keep</code> leaves the originals where they are. A store that cannot be read stops the migration before anything switches, so a Keychain prompt you decline cannot strand your values.</p>
    ${note("Keychain is the stronger bargain on a Mac", "It gates access per application, where a key file protects a vault that has been copied and not a process already running as you. It is a setting away in both directions, and neither store ever holds a value in plaintext.")}

    <h2 id="create">Create a secret</h2>
    ${code("shell", `synapse vault create work
synapse secret set work database DATABASE_URL
synapse secret list work`)}
    <p>When stdin is a terminal, <code>secret set</code> reads the value with a hidden prompt and asks for confirmation. When stdin is piped, it reads the stream and trims its final line ending. The value is never accepted as a command argument.</p>
    ${code("shell", `printf '%s' "$DATABASE_URL" | synapse secret set work database DATABASE_URL`)}
    ${note("Shell history boundary", "The safe prompt keeps the value out of history and process arguments. A piped command is only as safe as the command that produces its stdin; do not paste a value directly into a visible shell command.")}
    <p>Add <code>--global</code> to make the environment name available in every folder, or change an existing label later:</p>
    ${code("shell", `synapse secret set work registry NPM_TOKEN --global
synapse secret global work.registry off`)}

    <h2 id="copy">Get a value back</h2>
    <p>A value never appears on screen, in a log, or in an MCP response. The one way to retrieve one is onto the clipboard:</p>
    ${code("shell", `synapse secret copy work.database`)}
    <p>The command prints the reference it copied and never the value. In the desktop app the same thing is a <strong>Copy</strong> button on the secret's row. On macOS the value goes through <code>pbcopy</code> over stdin, never as a command argument; set <code>SYNAPSE_CLIPBOARD</code> to name a different command.</p>

    <h2 id="files">Scope files</h2>
    <p>From a project folder, create <code>.synapse.yaml</code>:</p>
    ${code("shell", `synapse scope init .`)}
    <p>The generated project template is valid but maps nothing:</p>
    ${code("yaml", `version: 1
scope: project
env: {}
deny: []`)}
    <p>Edit it to map names to existing references. Unknown fields are rejected.</p>
    ${code("yaml", `version: 1
scope: project
env:
  DATABASE_URL: work.database
  NPM_TOKEN: work.registry
deny:
  - PRODUCTION_TOKEN`)}
    <p>Use <code>synapse scope init path/to/folder --folder</code> for a nested folder scope. The <code>scope</code> field describes intent and appears in status output; resolution uses the file’s position in the ancestor path.</p>

    <h2 id="approval">Approval</h2>
    <p>A scope has no effect until its exact contents are approved. Synapse hashes the file and stores its canonical path plus digest in SQLite:</p>
    ${code("shell", `synapse status .
synapse allow
synapse scope status .`)}
    <p><code>synapse allow</code> approves the closest discovered scope. If several ancestor scopes exist, approve each from its own folder. <code>synapse scope trust [folder]</code> remains available when you want to approve an exact path directly.</p>
    <p>Changing even whitespace changes the digest. The scope then reports <strong>changed</strong> and stops applying until you inspect and approve it again. Invalid YAML and unknown references produce warnings.</p>
    <p>Both shell modes refuse an incomplete environment when any discovered scope is pending, changed, invalid, or references an unknown secret. Command-scoped mode refuses to launch; ambient mode unloads its managed values.</p>

    <h2 id="precedence">Precedence and deny</h2>
    <ol>
      <li>Global mappings load first.</li>
      <li>Synapse discovers every <code>.synapse.yaml</code> from the filesystem root toward the working folder.</li>
      <li>Approved files apply in that order, so the closest mapping replaces a broader mapping for the same environment name.</li>
      <li>A name in <code>deny</code> is removed and remains denied for all narrower scopes. A child scope cannot add it back.</li>
    </ol>
    <p>This lets a repository override a harmless global credential while permanently blocking a production credential inside a sensitive subtree.</p>

    <h2 id="modes">Choose an environment boundary</h2>
    <h3>Command scoped</h3>
    ${code("shell", `cd /path/to/project
synapse status .
synapse run -- cargo test
synapse run -- bun run deploy`)}
    <p><code>synapse run</code> resolves the current working folder, verifies every discovered scope, reads each selected value out of the vault, and sets it only on the new child process. Normal environment inheritance still applies; Synapse adds or replaces the resolved names. Your current shell is unchanged.</p>

    <h3>Ambient directory</h3>
    <p>Open <strong>Settings → Shell environments</strong> and choose <strong>Enable shell hook</strong>. Synapse detects the default shell, installs the CLI if needed, and manages a marked startup-file block. Open a new terminal, then allow the project:</p>
    ${code("shell", `cd /path/to/project
synapse allow`)}
    <p>For a temporary or manual installation, evaluate the matching hook directly:</p>
    ${code("shell", `# Add the matching line to your shell startup file.
eval "$(synapse hook zsh)"
# eval "$(synapse hook bash)"
# synapse hook fish | source`)}
    <p>The hook loads the approved environment when the shell enters the project, unloads it when the shell leaves, and reloads it after an approved change. If Synapse replaced a variable that already existed, it restores the original value instead of unsetting it. A hook never activates from global mappings alone: the directory must contain at least one approved discovered scope.</p>
    <p>Run <code>synapse deny</code> to revoke the closest scope. The next prompt unloads managed values. <code>synapse status .</code> reports whether ambient activation is ready, blocked, or inactive and whether the current shell has a hook installed.</p>
    ${note("Ambient mode has a wider boundary", "Every process launched from the activated shell can read the values. Prefer synapse run for a sensitive one-off command. Do not enable shell tracing such as set -x while the hook is evaluating environment changes, because tracing can print plaintext values.")}
    <p>MCP <code>vaultstatus</code> and CLI <code>status</code> report names, scope states, and warnings. They do not read or reveal values. To remove a value and its metadata, run <code>synapse secret forget work.database</code>. A vault can be deleted only after all of its secrets are forgotten.</p>
  `,
};

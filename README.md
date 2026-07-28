# Synaps

Synaps is a local memory service for coding tools. It runs in the macOS menu bar, stores durable context in SQLite, and exposes that memory through MCP.

## What works

- Native GPUI dashboard built with Guise.
- Menu-bar lifecycle: closing the dashboard hides it while Synaps keeps running.
- Full-text SQLite memory with `remember` and `recall` MCP tools.
- Searchable memory history with inspect, edit, delete, and guarded wipe controls.
- Detection and one-click user-level setup for Codex and Claude Code, including stale-path repair.
- Built-in Guise live-preview editor for global Markdown instructions.
- Built-in Guise code editor for global TOML and JSON configuration.
- Keychain-backed vaults with global, project, and folder scopes.
- Built-in YAML editor and digest approval for `.synaps.yaml` files.
- Command-scoped injection and GUI-managed zsh, bash, or fish directory activation.
- Safe `vaultstatus` MCP metadata that never returns secret values.
- Configurable Full, Balanced, and Lean recall responses for lower token use.
- System-following light and dark themes with explicit overrides.
- A complete `synaps` CLI and per-user installer in the app and platform menus.
- Idempotent managed instruction blocks that preserve user-owned content.
- Numbered database migrations, startup integrity checks, automatic backups, export, and restore.
- Validated, atomic configuration writes with recovery copies and setup rollback.

## Run

```sh
cargo run
```

The application creates `brain.db` in the platform application-data directory. Set `SYNAPS_DATA` to use another data directory during development, `SYNAPS_HOME` to isolate global-tool files, or `SYNAPS_DOCUMENT` to launch directly into a Markdown, TOML, or JSON file.

The MCP stdio entrypoint used by connected tools is:

```sh
synaps mcp
```

The Settings screen installs the CLI at `~/.local/bin/synaps` on macOS and Linux. A packaged app installs a small executable launcher so the Developer ID signature stays valid; move the app out of a mounted image before installing. Development binaries are copied atomically. An existing unrelated file is never overwritten. `SYNAPS_BIN` can select another destination.

Useful commands:

```sh
synaps status .
synaps vault create work
synaps secret set work database DATABASE_URL
synaps scope init .
synaps allow
synaps settings optimize balanced
synaps run -- cargo test
eval "$(synaps hook zsh)"
synaps memory list
synaps data check
synaps data export synapsbackup.db
```

`secret set` reads from a hidden terminal prompt or stdin; values are never accepted as command arguments. Run `synaps help` for the full command surface.

## Vault scopes

Synaps stores secret values in macOS Keychain. SQLite contains labels, references, global assignments, and approved file digests. A project file contains references only:

```yaml
version: 1
scope: project
env:
  DATABASE_URL: work.database
  SENTRY_AUTH_TOKEN: work.sentry
deny:
  - PRODUCTION_TOKEN
```

Create or edit `.synaps.yaml` from the Vaults screen, then approve its current contents. Any later edit invalidates that approval. Nested files are resolved from the filesystem root toward the working folder, so the closest approved scope overrides broader mappings. A denied name stays denied in narrower scopes.

Choose the environment boundary that fits the command. For one child process:

```sh
synaps run -- cargo test
```

For automatic loading in approved directories, open **Settings → Shell environments** and choose **Enable shell hook**. Synaps detects zsh, bash, or fish, installs the CLI if needed, and adds one managed block to the matching startup file. Start a new terminal afterward.

The equivalent manual setup is:

```sh
# zsh
eval "$(synaps hook zsh)"

# bash
eval "$(synaps hook bash)"

# fish
synaps hook fish | source
```

Run `synaps allow` after inspecting the closest `.synaps.yaml`. The hook loads it on entry, unloads it on exit or invalidation, and restores any value that existed before activation. Run `synaps deny` to revoke the closest scope. Ambient activation requires at least one approved discovered scope; global mappings alone do not activate it.

Settings reports the integration as enabled, missing, or modified. **Repair hook** replaces only the marked Synaps block. **Remove hook** removes that block; existing terminals keep the already-loaded hook until they close. Startup-file changes are backed up to a `.synapsbackup` sibling and written atomically.

The command-scoped child, or every process launched from an ambient shell, receives normal environment variables and can read their values. Prefer `synaps run` for sensitive one-off work. MCP exposes variable names and trust state only; it cannot inject values. The ambient integration works because the shell explicitly evaluates a quoted environment diff. Keep shell tracing disabled while it does so.

## Token optimization

Recall optimization is non-destructive: SQLite keeps the original memory, while MCP responses may compact whitespace, remove exact duplicate results, and apply a response budget.

- Full: up to 25 results, original formatting, no response budget.
- Balanced: up to 8 results and roughly 1,500 tokens.
- Lean: up to 4 results and roughly 700 tokens.

Token figures are estimates because tokenizers vary. Balanced is the default.

## Data safety

Synaps checks SQLite page integrity and foreign-key relationships before use, then applies numbered migrations at startup. A pre-migration backup is created in the application data folder before schema changes. Database files, sidecars, locks, and backups use owner-only permissions on Unix systems.

Use `synaps data export <file>` for a consistent portable snapshot. Restores validate the source first, preserve the current database as a recovery backup, and require the app and connected tools to be closed. The memory screen and CLI both require explicit confirmation before destructive wipes.

Structured files edited by Synaps are parsed before saving. Existing JSON, TOML, YAML, and instruction files are backed up to a `.synapsbackup` sibling, then replaced atomically while retaining their permissions and any dotfile symlink. Failed tool setup and CLI installation attempts restore their prior state. Connection detection reads the actual Codex TOML and Claude user JSON stores so deleted or moved Synaps executables can be repaired instead of appearing connected.

## macOS beta

The current distribution target is Apple-silicon macOS 13 or later. Build and sign the app on an Apple-silicon Mac with the Developer ID certificate installed:

```sh
scripts/release
```

This creates `dist/synaps.app` and `dist/synaps.zip`, verifies the hardened signature, and leaves the archive ready for Apple notarization. The notarization script accepts `APPLE_ID`, `APPLE_TEAM_ID`, and `APPLE_APP_PASSWORD` from the environment. It otherwise uses the `synaps` Keychain profile. To store that profile, enter the app-specific password only at the secure prompt:

```sh
xcrun notarytool store-credentials synaps --apple-id you@example.com --team-id XJDC46F35X
scripts/notarize
```

The notarization script submits the archive, staples and validates the ticket, runs Gatekeeper assessment, and recreates the distributable archive with the stapled app.

## Website

The GitHub Pages site is generated from the focused TypeScript, CSS, and content modules in `website/`:

```sh
bun run site
bun run sitecheck
bun run serve
```

The build writes the static artifact to `site/`. The check verifies every local link, anchor, asset, filename, canonical URL, sitemap entry, required landing-page claim, and complete desktop-app, CLI, and MCP documentation coverage. The local server mirrors the project path at `http://127.0.0.1:4173/synapse/`.

`.github/workflows/pages.yml` rebuilds and checks the site on `main`, then publishes `site/` through GitHub Pages at `https://wess.io/synapse/`. Repository and release links use `wess/synapse`.

## Verify

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo audit
```

The test suite uses isolated homes and data folders. It covers MCP stdio, memory and database lifecycle commands, setup rollback, clean CLI installation, conflict protection, ambient approval and invalidation, original-value restoration, and a temporary macOS Keychain secret that is deleted before the test exits.

# Extracting `crates/synapsecore`

A plan for pulling the memory engine out of `crates/synapse` into a library crate that
builds on headless Linux with no gpui, no guise, no Keychain, and no rmcp.

Analysis only. Nothing in this document has been applied, and no build was run — the
`../guise` path dependency is not resolvable here. Every claim below is drawn from
reading the sources named; the two places where I could not verify something are
marked **unverified**.

## 1. Why

All durable logic lives in one binary crate. `crates/synapse/Cargo.toml` pulls
`gpui 0.2.2`, `guise-ui` (path dep on `../guise/crates/guise`), `rmcp 2.2.0`,
`security-framework 3.7`, `cocoa`, `tray-icon`, `rpassword`, and `directories` into the
same compilation unit as the SQLite storage. An embedder that wants `remember`/`recall`
has to link a GPUI desktop app and check out a sibling theming repository.

The good news, from actually mapping the imports: **the coupling is shallow.** gpui and
guise appear only under `src/ui/`. rmcp appears only in `src/mcp/server.rs` and
`src/mcp/mesh.rs`. `security_framework` appears only in `src/vault/keychain.rs` (40
lines). `directories` appears only in `src/files/index.rs`. `rpassword` appears once, at
`src/cli/command.rs:502`. `cocoa`/`tray_icon`/`async_channel` appear only in
`src/ui/statusbar.rs`. Nothing in `brain/`, `database/`, `vault/store.rs`,
`vault/scope.rs`, or `relay/store.rs` touches any of them.

The extraction is therefore mostly a move, not a rewrite. The design work is in four
places: where secret *values* are read, where the mesh's process-orchestration half is
cut off from its database half, who owns `user_version`, and how a caller says where
`brain.db` is.

## 2. Actual dependency edges

### External crates, by file

| Crate | Files that reference it |
| --- | --- |
| `gpui` | `ui/{agentrow,buffer,clibanner,dashboard,document,header,index,memories,menu,mesh,settings,skills,statusbar,summary,theme,vaults}.rs` — **only `ui/`** |
| `guise` (`guise-ui`) | `ui/{clibanner,skills,mesh,header,theme,buffer,agentrow,document,summary,…}.rs` — **only `ui/`** |
| `rmcp` | `mcp/server.rs`, `mcp/mesh.rs` |
| `security-framework` | `vault/keychain.rs` |
| `cocoa`, `tray-icon`, `async-channel` | `ui/statusbar.rs`, `ui/index.rs` |
| `directories` | `files/index.rs` (`BaseDirs`, in `home()` and `data()`) |
| `rpassword` | `cli/command.rs:502` |
| `libc` | `relay/process.rs` |
| `chrono` | `crashes.rs`, `ui/memories.rs` |
| `schemars` | `brain/model.rs`, `imports/model.rs`, `mcp/model.rs`, `relay/model.rs`, `vault/model.rs`, `skill/{install,model}.rs` |
| `serde-saphyr` | `vault/scope.rs`, `files/validate.rs`, `imports/markdown.rs`, `skill/model.rs` |
| `toml` | `relay/{role,team}.rs`, `agent/config.rs`, `files/validate.rs` |
| `fs2` | `database/permission.rs` |
| `sha2` | `brain/ingest.rs`, `vault/scope.rs`, `relay/launch.rs`, `skill/model.rs`, `cli/install.rs` |
| `sqlx` | `database*`, `brain/{model,settings,store,ingest}.rs`, `vault/{model,store}.rs`, `relay/{model,store}.rs`, `imports/{model,codex}.rs`, `skill/receipts.rs` |
| `tokio` | `relay/{bus,store,worker}.rs`, `mcp/*`, all of `cli/*`, `brain/{store,ingest}.rs` (tests only), `vault/{scope,store}.rs` (tests only) |

### Cross-module edges inside the crate that matter for the cut

- `brain/scope.rs:25` → `crate::vault::CONFIG` (`".synapse.yaml"`). The only brain→vault
  edge in the whole crate, and it is one `&'static str`.
- `brain/model.rs:41` → `crate::brain::projectroot` (`MemoryScope::project`).
- `brain/ingest.rs` → `crate::imports::{ImportBatch, ImportCandidate, …}`; and
  `brain/store.rs::deletememory` / `wipememories` delete from `memoryorigin` and
  `importbatch`. Memory and imports are one table family, not two.
- `brain/store.rs`, `vault/store.rs`, `relay/store.rs`, `skill/receipts.rs` each call
  `crate::database::open` (or `glance`) independently — four opens of one file per
  command, which is exactly why the `VERIFIED` memo in `database.rs:81` exists.
- `relay/launch.rs:12` → `crate::agent::{Agent, Kind}` (the Claude/Codex catalog).
  `relay/layer.rs:48`, `relay/role.rs`, `relay/team.rs` → `crate::files::data()` plus
  `include_str!("../../assets/roles/*.toml")`.
- `vault/run.rs:32,50` and `vault/shell.rs` → `crate::files::database()` and
  `crate::vault::getsecret` — the only two call sites in the crate that read a secret
  value.

### Env / real-path reaches

Only `files/index.rs` reads `SYNAPSE_HOME` (line 9) and `SYNAPSE_DATA` (line 18), and
only it constructs `BaseDirs`. Everything else derives from `files::data()` /
`files::database()` / `files::home()` / `files::soul()`. `SYNAPSE_PROJECT_DIR` is read
in `mcp/server.rs:100,130` and `cli/session.rs:310` and written in `relay/launch.rs:124`.
`SYNAPSE_BIN` is read in `cli/install.rs:15`; `CODEX_HOME` in `agent/catalog.rs:31`;
`SYNAPSE_SHELL_*` in `cli/shell.rs` and `vault/shell.rs`; `SYNAPSE_PAGE` and
`SYNAPSE_DOCUMENT` in `ui/dashboard.rs`.

**None of these is in a file that belongs in core.** The path resolution problem solves
itself if `files/index.rs`'s `home`/`data`/`database`/`soul` stay in the app.

## 3. Module classification

Key: **A** moves verbatim, **B** moves with surgery, **C** stays in `crates/synapse`.

### `database/`

| File | Verdict | Notes |
| --- | --- | --- |
| `database.rs` | **A** | `open`, `glance`, `opened`, `connect`, `readonly`, `integrity`, `pages`, `relationships`, `version`, `Identity`, `VERIFIED`, `unverified`, `verified`. sqlx + std only. |
| `database/permission.rs` | **A** | `fs2` + `cfg(unix)` mode bits. Compiles on Linux as-is. |
| `database/backup.rs` | **A** | `KEEP = 5`, `create`, `folder`, `snapshots`, `stamp`, `rotate`, `vacuum`. |
| `database/lifecycle.rs` | **A** | `Report`, `check`, `export`, `restore`, `replace`. |
| `database/migration.rs` | **A** | `LATEST = 7`, `MIGRATIONS`, `run`. See §4.3 — core owns this and the app owns none. |
| `database/tests.rs` | **B** | Moves, but `orphan()` (line 129) inserts into `secret`. That stays valid only because the vault tables remain in core's single migration list. If you ever namespace migrations, this test breaks first — it is the canary. |

### `brain/` → `synapsecore::memory`

| File | Verdict | Surgery |
| --- | --- | --- |
| `brain/optimize.rs` | **A** | `recall`, `compact`, `truncate`. Pure. |
| `brain/settings.rs` | **A** | `read`, `write`, `mesh`, `writemesh`, `value`, `writevalue`. |
| `brain/store.rs` | **A** | `Brain`, `rememberscoped`, `recallscoped`, `search`, `searchscoped`, `reach`, `memory`, `updatememory`, `updatememoryscoped`, `deletememory`, `wipememories`, `settings`, `setoptimization`, `mesh`, `setmesh`, `preference`, `setpreference`, `stats`, `STOPWORDS`, `search_expression`, `stopword`. Every FTS5 subtlety travels with it (§7.5). |
| `brain/scope.rs` | **B** | Replace `crate::vault::CONFIG` with a core-local `pub const CONFIG: &str = ".synapse.yaml"`, re-exported from `synapsecore` root. `projectroot` otherwise verbatim. |
| `brain/model.rs` | **B** | Split. `Memory`, `MemoryScope`, `Optimization`, `Settings`, `Stats` → core, with `#[derive(JsonSchema)]` changed to `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]`. `RememberRequest`, `RememberResponse`, `RecallRequest`, `RecallResponse` → **C**, they move to `crates/synapse/src/mcp/model.rs`: they are MCP wire shapes with model-facing doc comments, not engine types. |
| `brain/ingest.rs` | **A** | `importpreview`, `importmemories`, `importbatches`, `importbatch`, `undoimport`, `preview`, `digest`, `now`. Must be in core because `wipememories`/`deletememory` already delete from the same tables. |

### `imports/`

| File | Verdict | Notes |
| --- | --- | --- |
| `imports/model.rs` | **B** | Moves to core (it is the input type for `importpreview`), schemars derives feature-gated as above. `ImportProvider`'s `Claude`/`Codex`/`Markdown` variants come along; they are only a stored discriminator in the `provider` TEXT column. Turning it into a `String` would be cleaner for third-party embedders but is a schema-visible change — not worth it now. |
| `imports/secret.rs` | **A** | `warning` — the credential-shaped-content heuristic. Belongs in core: the invariant "imports never pull in credential-shaped content" must hold for an embedder feeding its own candidates. Change `pub(crate) use` to `pub`. |
| `imports/claude.rs` | **C** | Reads `~/.claude`. Portable, but it is knowledge about another tool's on-disk layout, which is the app's business. |
| `imports/codex.rs` | **C** | Same, plus it opens Codex's own SQLite history with its own `SqlitePoolOptions`. |
| `imports/markdown.rs` | **C** | Same. |

### `vault/`

| File | Verdict | Notes |
| --- | --- | --- |
| `vault/store.rs` | **A** | `VaultStore` and all of it. Never touches Keychain — it stores names, env names, and `account` references, plus the `trust` digests. |
| `vault/scope.rs` | **A** | `CONFIG`, `template`, `templatefor`, `discover`, `read`, `resolve`, `apply`. `serde_saphyr` + `sha2` only. |
| `vault/model.rs` | **B** | Split. `Vault`, `Secret`, `ScopeConfig`, `ScopeKind`, `ScopeState`, `Resolved` → core. `VaultStatusRequest`, `VaultStatusResponse`, `VaultScopeResponse` and the `From<ScopeState>` impl → **C**, to `crates/synapse/src/mcp/model.rs`. |
| `vault/keychain.rs` | **C** | `security-framework`. Stays, unchanged, including the `cfg(not(target_os = "macos"))` bail arms. |
| `vault.rs`'s `setsecret`/`getsecret`/`deletesecret` | **C** | Thin wrappers over `keychain`. |
| `vault/run.rs` | **C** | `run` spawns a child process; `environment`/`names` call `crate::files::database()` and `getsecret`. Rewritten in step 3 to delegate to `synapsecore::vault::values` / `names`. |
| `vault/shell.rs` | **C** | zsh/bash/fish hook text, `SYNAPSE_SHELL_*` state, calls `getsecret`. |

### `relay/`

| File | Verdict | Notes |
| --- | --- | --- |
| `relay/store.rs` | **B** | → `synapsecore::mesh`, behind `feature = "mesh"`. `Mesh`, `register`, `placeholder`, `touch`, `setstatus`, `statusof`, `insert`, `pending`, `ack`, `subscribe`, `unsubscribe`, `subscriptions`, `channels`, `agents`, `forget`, `feed`, `saveworker`, `workerstate`, `deleteworker`, `workers`, `worker`, `validname`, `LIVEWINDOW`, `RETENTION`, `MAXBACKLOG`. Surgery: `validname` becomes `pub` (used by `relay/launch.rs:71` which stays in the app), and line 333's `serde_json::to_string(&worker.arguments)` is the only reason core needs `serde_json` — gate it on the `mesh` feature. |
| `relay/model.rs` | **B** | → core. `Message`, `MessageKind`, `Registration`, `AgentView`, `ChannelView`, `WorkerView`. schemars derives feature-gated. |
| `relay/bus.rs` | **A** | → core. `PARKSECONDS`, `CLIENTIDLEFLOOR`, `PROGRESSSECONDS`, `TICK`, `HEARTBEAT`, the two `const _: () = assert!` blocks, `deliver`, `awaitmessages`, `ack`, `reportstatus`, `awaitstatus`. Needs `tokio` with `time` only. |
| `relay/launch.rs` | **C** | Depends on `crate::agent::{Agent, Kind}`, writes generated `--mcp-config` files, resolves `crate::files::home()`. Tool orchestration. |
| `relay/harness.rs` | **C** | The model-facing prompt text. Ships with the app's guidance, same as `instructions.rs`. |
| `relay/worker.rs` | **C** | Spawns processes, writes logs under `crate::files::data()`, `Supervisor`, `reapstrays`, `MAXLOG` tail bound. |
| `relay/process.rs` | **C** | `libc::kill`. |
| `relay/role.rs`, `relay/team.rs`, `relay/layer.rs` | **C** | `include_str!("../../assets/…")`, `toml`, and `crate::files::data()` for the user layer. |
| `relay.rs::directory()` | **C** | `crate::files::data()?.join("relay")`. |

### `files/`

| File | Verdict | Notes |
| --- | --- | --- |
| `files/index.rs` | **C** | All of it. `home`/`data`/`database`/`soul` are the env + `directories` resolution core must not have; `read`/`write`/`ensure`/`writetarget`/`backuppath` guard *other tools'* config files; `reveal` shells out to `open`/`explorer`/`xdg-open`. |
| `files/atomic.rs` | **C** | Only used by `files::write` and `Snapshot`. |
| `files/validate.rs` | **C** | Only used by `files::write`. Keeping it out of core keeps `toml` and `serde_json` out of core's non-optional deps. |
| `files/rollback.rs` (`Snapshot`) | **C** | See §4.5. |
| `files.rs::scopeddata` | **C** | The `#[cfg(test)]` process-wide `SYNAPSE_DATA` mutex. Must stay next to the thing it guards. |

### Everything else

`agent/*`, `cli/*`, `crashes.rs`, `instructions.rs`, `mcp/*`, `shellsetup.rs`, `skill/*`,
`ui/*`, `main.rs` — all **C**. Note `skill/receipts.rs` calls `crate::database::open`; it
keeps working through core's public `database::open`, and the `skillinstall` table stays
in core's migration list (§4.3).

## 4. The awkward cases

### 4.1 `vault` — splitting the store from the keyring

**The coupling is already thinner than it looks.** Grep the crate for the three keychain
functions and you get exactly two non-UI, non-CLI call sites:
`vault/run.rs:42` (`environment`) and `vault/shell.rs:60` (`changes`). Everything else in
the vault — `VaultStore`, `resolve`, `discover`, `read`, `trust`/`untrust`/`digest`,
`findsecret`, `globalsecrets` — deals only in names, env var names, and `account`
strings. `Resolved.env` is `BTreeMap<String, Secret>`, and `Secret` has no value field.

So the split is: **core gets everything up to and including `Resolved`; the value read is
a callback supplied by the caller.** No trait.

```rust
// synapsecore::vault

/// The variable names a child launched in this scope would carry. No secret
/// backend is consulted, so this is what a preview prints.
pub fn names(resolved: &Resolved) -> Result<Vec<String>>;

/// The variables a child should carry, with values read through `read`.
///
/// Refuses on any scope warning rather than returning a partial environment:
/// a tool that can run a shell is never handed a half-resolved environment.
pub fn values<F>(resolved: &Resolved, read: F) -> Result<Vec<(String, String)>>
where
    F: Fn(&str) -> Result<String>;
```

`crates/synapse` calls `values(&resolved, vault::getsecret)`. A Linux embedder passes
`|account| std::env::var(account).context("no value")`, or a closure over
`secret-service`, or `|_| anyhow::bail!("no secret backend on this platform")` — which is
exactly what `vault/keychain.rs`'s non-macOS arms already do, just relocated to the
caller.

This is also a small **strengthening**: the
`anyhow::ensure!(resolved.warnings.is_empty(), …)` guard currently lives in `run.rs:34`
and `run.rs:52`, duplicated. Moving it inside `values` and `names` makes it structurally
impossible for a new embedder to resolve an incomplete scope, which is a product
invariant, not a convenience.

For embedders that also want to *write* secrets (i.e. use `createsecret` and store the
value), offer data rather than a trait, matching the repo's "prefer data and free
functions" rule:

```rust
/// A secret backend, as three functions. Core never constructs one and never
/// stores one; a caller passes it to the two helpers that need it.
pub struct Keyring {
    pub read: fn(&str) -> Result<String>,
    pub write: fn(&str, &str) -> Result<()>,
    pub forget: fn(&str) -> Result<()>,
}
```

Ordering invariant to document beside `createsecret`/`deletesecret`: today
`cli/command.rs:337–345` writes the Keychain entry *after* the row and rolls the row back
if the write fails, and lines 268–269 delete the Keychain entry *before* the row. Core
cannot enforce that because core does not hold the backend; state it in the doc comment
on `vault::createsecret` and `vault::deletesecret`.

Feature name: `vault`, on by default. With it off, `synapsecore` drops `serde-saphyr` and
`sha2`-for-scopes and becomes memory-only.

### 4.2 `relay` — core, app, or third crate?

**Split it: the four tables go to core behind `feature = "mesh"`, the process
orchestration stays in the app. Do not make a third crate.**

Arguments from the code, not from taste:

- The bus *is* the database. `relay/store.rs` and `relay/bus.rs` are sqlx plus
  `tokio::time` and nothing else. `Mesh::open` calls `crate::database::open`
  (`relay/store.rs:49`). The `meshagent`/`meshsub`/`meshmessage`/`meshworker` tables are
  in migration 3 of the same list as `memory`. A separate crate would have to take a
  `SqlitePool` from core anyway, and would exist only to hold a manifest.
- The rest of `relay/` is not the mesh, it is *tool launching*. `relay/launch.rs` imports
  `crate::agent::{Agent, Kind}`; `relay/role.rs` and `relay/team.rs`
  `include_str!("../../assets/roles/…")` from the app's asset folder; `relay/layer.rs`
  reads `crate::files::data()`; `relay/worker.rs` spawns children and writes logs under
  the data dir; `relay/process.rs` is `libc::kill`. Moving any of that into core would
  drag the Claude/Codex catalog, embedded TOML assets, and data-dir layering into a
  library whose whole premise is not knowing where files live.
- Cut line is clean at the type level. `relay/worker.rs` uses `Mesh` and `WorkerView`,
  both of which become core types; `relay/launch.rs` uses `store::validname`, which just
  needs to become `pub`. `mcp/mesh.rs` (the `#[tool_router]`) calls `Mesh` methods plus
  `bus::{deliver, awaitmessages, ack, …}` — all core after the split — and holds
  `Supervisor`, which stays.

What an embedder gets: `nora` or `tandem` can register on the mesh, send, post, park on
`wait`, and read the roster, without linking a process supervisor or the tool catalog.
Spawning workers stays a `synapse` responsibility, which is right — the harness prompt,
the `--mcp-config` generation, and the permission-prompt policy are product decisions.

### 4.3 `database` — who owns the migration list

**One list, in core, keyed by `user_version`, exactly as it is today. Not namespaced.
The app owns no migrations at all.**

Reasons, all specific to this repo:

1. `user_version` is one 32-bit slot per SQLite file. `migration::run` reads it once
   (`migration.rs:152`) and bails with *"database version {current} is newer than this
   Synapse release supports"* if it is ahead. Two independent owners appending to one
   counter cannot both be right. Namespacing would mean a `schemaversion(namespace,
   version)` table, a rewrite of `run`, and a bootstrap migration applied to databases
   already sitting at version 7 in the field.
2. The shipped migrations are frozen bytes. CLAUDE.md's rule is "append a new entry and
   bump `LATEST`, never edit a shipped one". Migration 1 creates `memory`, `setting`,
   `vault`, `secret`, `globalenv`, and `trust` in one statement list. You cannot retro-fit
   per-namespace lists onto 1..7 without editing shipped history.
3. The tables are already entangled across module boundaries and the code depends on it:
   `brain/store.rs::wipememories` deletes from `memoryorigin` and `importbatch`;
   `database/tests.rs::orphan` inserts a broken row into `secret` specifically to prove
   `foreign_key_check` fires. `skillinstall` (migration 4) is an app concept whose table
   is already in the shared list.

**How an embedder that only wants memory avoids the vault and mesh tables: it doesn't,
and that is the correct answer.** Six empty tables and three indexes cost a few hundred
bytes of `sqlite_master` and zero query time. What the features gate is *code*, not
*schema*:

- `synapsecore` always applies `MIGRATIONS[1..=LATEST]`, whatever features are on.
- `feature = "vault"` decides whether `VaultStore` and `resolve` are compiled.
- `feature = "mesh"` decides whether `Mesh` and the bus are compiled.

That keeps one `LATEST` and one file format, which matters concretely: the desktop app,
`synapse mcp`, and an embedded `nora` may all open the same `~/…/synapse/brain.db`. A
feature-gated *schema* would let a memory-only embedder create a database the desktop app
then refuses, because `run` would see `user_version` below `LATEST` and try to apply
migration 3 on top of a file that never had migrations 1–2 in the same shape.

Rule to write into CLAUDE.md: **new tables are appended to `synapsecore`'s
`MIGRATIONS`, and `LATEST` lives in `synapsecore`. `crates/synapse` never defines a
migration.** If the app grows a table, it grows core's list — which is what already
happened for `skillinstall`.

### 4.4 Path and env resolution

**Core takes a `&Path` to the database file and nothing else. There is no config
struct, because there is nothing else to configure.**

Everything env-dependent is in `files/index.rs`: `SYNAPSE_HOME` (line 9), `SYNAPSE_DATA`
(line 18), `BaseDirs::new()`. That file stays in the app. `crates/synapse/src/files.rs`
becomes a facade so the ~80 `crate::files::write(…)` / `crate::files::data()` call sites
do not move:

```rust
// crates/synapse/src/files.rs  (unchanged public surface)
mod atomic;
mod index;
mod rollback;
mod validate;

pub use index::{data, database, home, read, reveal, soul, write};
pub(crate) use atomic::copy as atomiccopy;
pub(crate) use rollback::Snapshot;
```

Core's entry point:

```rust
pub async fn open(path: &Path) -> Result<Store>;
pub async fn glance(path: &Path) -> Result<Store>;
```

The path *is* the injection. Derived locations stay derived, as they already are:
`backup::folder(database)` is `database.parent()?.join("backups")` (`backup.rs:34`), and
`permission::sidecar` / `lockpath` derive from the same path. Core never calls
`std::env::var` — that becomes a property worth a test:

```rust
#[test]
fn core_reads_no_environment_variables() { /* grep-style assertion over the crate */ }
```

...or, more practically, a CI grep: `! rg -q 'env::var' crates/synapsecore/src`. Cheap
and it will catch the regression the day someone adds a "convenient default".

For an embedder that wants Synapse's *own* default location (nora and merlin will), the
app should expose it rather than core:

```rust
// crates/synapse — or a tiny `synapsepaths` crate later if more than one embedder wants it
pub fn database() -> Result<PathBuf>;  // SYNAPSE_DATA or BaseDirs::data_local_dir()/synapse/brain.db
```

I deliberately do **not** propose putting that in core: the moment core knows a default
path, an embedder gets it by accident, and `SYNAPSE_DATA` stops being the single lever
that makes `tests/cli.rs` and `tests/mcp.rs` hermetic.

### 4.5 `files::write` and `files::Snapshot`

**App-only. Both of them.**

I checked every caller. `files::write` / `files::read` / `files::ensure` are referenced
from `agent/guidance.rs`, `agent/hooks.rs`, `cli/command.rs`, `cli/install.rs`,
`instructions.rs`, `relay/launch.rs`, `relay/layer.rs`, `relay/role.rs`,
`shellsetup.rs`, `skill/library.rs`, `ui/dashboard.rs`. `Snapshot::capture` is called
from `agent/guidance.rs`, `agent/setup.rs`, `cli/install.rs`, `ui/dashboard.rs`.

**Not one of them is in a module bound for core.** Nothing in the memory engine writes a
user-facing file: `database/lifecycle.rs` uses `fs::rename` directly, `backup.rs` uses
`VACUUM INTO`, and `vault/scope.rs` only reads `.synapse.yaml`.

The purpose of `files::write` confirms it: it exists to guard *other tools'* configuration
— parse-by-extension JSON/TOML/YAML validation (`files/validate.rs`), a `.synapsebackup`
sidecar, atomic replace, permission preservation, and writing *through* symlinks. That is
the "never overwrite user-owned configuration" invariant, and its subject is
`~/.claude/settings.json` and `~/.codex/config.toml`, not `brain.db`.

Keeping them out also keeps `toml`, `serde_json`, and `serde-saphyr`-for-validation out
of core's dependency list, which is the difference between core having 6 dependencies and
9.

If a future embedder wants the atomic-write behaviour, it is a candidate for its own
tiny crate — not a reason to put it in the memory engine.

## 5. Public API of `synapsecore`

Naming follows the repo: lowercase identifiers with no underscores, `CamelCase` types,
free functions over data, `anyhow` with lowercase prose context.

### The handle

```rust
// synapsecore/src/lib.rs

/// One open database: a pool, the path it came from, and the shared file lock
/// held for as long as the handle lives.
pub struct Store { /* pool: SqlitePool, path: PathBuf, lock: Arc<File> */ }

/// Open verified, migrated, and owner-only. Reads every page once per process
/// per unchanged file.
pub async fn open(path: &Path) -> Result<Store>;

/// Open to report on. Identical to `open` minus the whole-file scan — for
/// callers that redraw on every turn.
pub async fn glance(path: &Path) -> Result<Store>;

pub async fn close(store: Store);

/// The `.synapse.yaml` scope file name, which is also one of the two markers
/// `projectroot` walks for.
pub const CONFIG: &str = ".synapse.yaml";

/// Re-exported so an embedder and this crate cannot end up holding two
/// incompatible `SqlitePool` types from two sqlx versions.
pub use sqlx;
```

### `synapsecore::memory`

```rust
pub struct Memory { pub id: i64, pub body: String, pub source: String,
                    pub scope: MemoryScope, pub project: String, pub created: i64 }

pub enum MemoryScope { Global, Project }
impl MemoryScope {
    pub fn value(self) -> &'static str;
    pub fn project(self, path: Option<&Path>) -> Result<String>;
}

pub enum Optimization { Full, Balanced, Lean }
impl Optimization {
    pub fn name(self) -> &'static str;
    pub fn value(self) -> &'static str;
    /// A per-call budget can only *shrink* the configured one, never grow it.
    pub fn constrained(self, requested: Option<Self>) -> Self;
}

pub struct Settings { pub optimization: Optimization,
                      pub resultlimit: u32,
                      pub characterbudget: Option<usize> }
impl From<Optimization> for Settings { /* 25/None, 8/6000, 4/2800 */ }

pub struct Stats { pub entries: i64, pub bytes: u64 }

/// What `recall` answers with: the budget it actually applied, and the result.
pub struct Recalled { pub settings: Settings, pub memories: Vec<Memory> }

pub async fn remember(store: &Store, body: &str, source: Option<&str>,
                      scope: MemoryScope, project: Option<&Path>) -> Result<i64>;

/// Global memory plus this project's, never another project's. `budget` can
/// only reduce the store's configured optimization.
pub async fn recall(store: &Store, query: &str, limit: u32,
                    budget: Option<Optimization>, project: Option<&Path>)
    -> Result<Recalled>;

/// Unscoped search across everything, for an inspection surface.
pub async fn search(store: &Store, query: &str, limit: u32) -> Result<Vec<Memory>>;

pub async fn read(store: &Store, id: i64) -> Result<Option<Memory>>;
pub async fn update(store: &Store, id: i64, body: &str, source: Option<&str>)
    -> Result<Option<Memory>>;
pub async fn updatescoped(store: &Store, id: i64, body: &str, source: Option<&str>,
                          scope: MemoryScope, project: Option<&Path>)
    -> Result<Option<Memory>>;
pub async fn delete(store: &Store, id: i64) -> Result<Option<Memory>>;
pub async fn wipe(store: &Store) -> Result<u64>;

/// How many memories a recall from `project` could draw on. The same scope
/// rule `recall` uses, so a reported number is a reachable number.
pub async fn reach(store: &Store, project: Option<&Path>) -> Result<i64>;
pub async fn stats(store: &Store) -> Result<Stats>;

pub async fn settings(store: &Store) -> Result<Settings>;
pub async fn setoptimization(store: &Store, optimization: Optimization) -> Result<()>;
pub async fn preference(store: &Store, key: &str) -> Result<Option<String>>;
pub async fn setpreference(store: &Store, key: &str, value: &str) -> Result<()>;

/// The project `path` belongs to: closest ancestor with `.git` or
/// `.synapse.yaml`, or the folder itself. `Ok(None)` — not an error — for a
/// path that does not resolve.
pub fn projectroot(path: &Path) -> Result<Option<PathBuf>>;
```

### `synapsecore::imports`

```rust
pub enum ImportProvider { Claude, Codex, Markdown }
pub enum ImportStatus { Ready, Existing, Flagged }
pub struct ImportCandidate { pub provider: ImportProvider, pub externalid: String,
                             pub body: String, pub source: String,
                             pub scope: MemoryScope, pub project: String,
                             pub path: PathBuf, pub created: i64,
                             pub warning: Option<String> }
pub struct ImportItem   { /* … */ }
pub struct ImportPreview{ /* … */ }
pub struct ImportBatch  { /* … */ }
pub struct ImportReport { /* … */ }

pub async fn preview(store: &Store, provider: ImportProvider,
                     candidates: Vec<ImportCandidate>) -> Result<ImportPreview>;
pub async fn apply(store: &Store, preview: ImportPreview, includeflagged: bool)
    -> Result<ImportReport>;
pub async fn batches(store: &Store) -> Result<Vec<ImportBatch>>;
pub async fn batch(store: &Store, id: i64) -> Result<Option<ImportBatch>>;
pub async fn undo(store: &Store, id: i64) -> Result<u64>;

/// Whether this text looks like a credential, and why. Nothing flagged is ever
/// imported without an explicit opt-in.
pub fn warning(body: &str) -> Option<String>;
```

### `synapsecore::vault` *(feature `vault`)*

```rust
pub struct Vault  { pub id: i64, pub name: String, pub created: i64 }
pub struct Secret { pub id: i64, pub vaultid: i64, pub vault: String,
                    pub name: String, pub env: String, pub account: String,
                    pub global: bool, pub created: i64 }   // no value field, by design
pub struct ScopeConfig { pub version: u32, pub scope: ScopeKind,
                         pub env: BTreeMap<String, String>,
                         pub deny: BTreeSet<String> }
pub enum ScopeKind { Project, Folder }
pub struct ScopeState { pub path: PathBuf, pub kind: ScopeKind, pub trusted: bool,
                        pub changed: bool, pub env: Vec<String>,
                        pub denied: Vec<String>, pub error: Option<String> }
pub struct Resolved { pub env: BTreeMap<String, Secret>,
                      pub scopes: Vec<ScopeState>, pub warnings: Vec<String> }

pub fn discover(folder: &Path) -> Result<Vec<PathBuf>>;      // root → leaf
pub fn readscope(path: &Path) -> Result<(ScopeConfig, String)>;  // config + sha256
pub fn template() -> &'static str;
pub fn templatefor(kind: ScopeKind) -> &'static str;

pub async fn resolve(store: &Store, folder: &Path) -> Result<Resolved>;

pub async fn vaults(store: &Store) -> Result<Vec<Vault>>;
pub async fn createvault(store: &Store, name: &str) -> Result<Vault>;
pub async fn deletevault(store: &Store, id: i64) -> Result<Option<Vault>>;
pub async fn secrets(store: &Store, vaultid: i64) -> Result<Vec<Secret>>;
pub async fn createsecret(store: &Store, vaultid: i64, name: &str, env: &str,
                          global: bool) -> Result<Secret>;
pub async fn deletesecret(store: &Store, id: i64) -> Result<Option<Secret>>;
pub async fn setglobal(store: &Store, id: i64, global: bool) -> Result<()>;
pub async fn globalsecrets(store: &Store) -> Result<Vec<Secret>>;
pub async fn findsecret(store: &Store, reference: &str) -> Result<Option<Secret>>;
pub async fn trust(store: &Store, path: &Path, digest: &str) -> Result<()>;
pub async fn untrust(store: &Store, path: &Path) -> Result<bool>;
pub async fn digest(store: &Store, path: &Path) -> Result<Option<String>>;

pub fn names(resolved: &Resolved) -> Result<Vec<String>>;
pub fn values<F>(resolved: &Resolved, read: F) -> Result<Vec<(String, String)>>
where F: Fn(&str) -> Result<String>;
pub struct Keyring { pub read:   fn(&str) -> Result<String>,
                     pub write:  fn(&str, &str) -> Result<()>,
                     pub forget: fn(&str) -> Result<()> }
```

### `synapsecore::mesh` *(feature `mesh`)*

```rust
pub struct Registration { pub name: String, pub role: String,
                          pub capabilities: String, pub project: String,
                          pub tool: String, pub human: bool }
pub struct Message   { pub id: i64, pub sender: String, pub kind: MessageKind,
                       pub target: Option<String>, pub body: String, pub created: i64 }
pub enum MessageKind { Direct, Channel, Broadcast }
pub struct AgentView  { /* name, role, status, note, project, tool, human,
                          registered, online, channels, seen */ }
pub struct ChannelView{ pub channel: String, pub subscribers: i64 }
pub struct WorkerView { /* name, role, status, process, restarts, keepalive,
                          directory, log, supervisor */ }

pub const PARKSECONDS: u64 = 240;
pub const PROGRESSSECONDS: u64 = 30;

pub async fn register(store: &Store, registration: &Registration) -> Result<()>;
pub async fn placeholder(store: &Store, name: &str) -> Result<()>;
pub async fn touch(store: &Store, name: &str) -> Result<()>;
pub async fn forget(store: &Store, name: &str) -> Result<()>;
pub async fn agents(store: &Store) -> Result<Vec<AgentView>>;
pub async fn channels(store: &Store) -> Result<Vec<ChannelView>>;
pub async fn subscribe(store: &Store, agent: &str, channel: &str) -> Result<()>;
pub async fn unsubscribe(store: &Store, agent: &str, channel: &str) -> Result<()>;
pub async fn deliver(store: &Store, from: &str, kind: MessageKind,
                     target: Option<&str>, body: &str) -> Result<i64>;
/// Parks up to `PARKSECONDS`; returns without advancing the cursor. Delivery
/// is at-least-once: `ack` once the reply has been built.
pub async fn awaitmessages(store: &Store, name: &str, block: bool)
    -> Result<Vec<Message>>;
pub async fn ack(store: &Store, name: &str, id: i64) -> Result<()>;
pub async fn reportstatus(store: &Store, name: &str, status: &str,
                          note: Option<&str>) -> Result<()>;
pub async fn awaitstatus(store: &Store, name: &str) -> Result<(String, String)>;
pub fn validname(value: &str) -> Result<String>;
```

### `synapsecore::data`

```rust
pub struct Report { pub path: String, pub version: i64, pub integrity: &'static str }

pub async fn check(database: &Path) -> Result<Report>;
pub async fn export(database: &Path, target: &Path) -> Result<()>;
pub async fn restore(database: &Path, source: &Path) -> Result<Option<PathBuf>>;
pub fn backupfolder(database: &Path) -> Result<PathBuf>;
pub fn snapshots(folder: &Path) -> Vec<PathBuf>;
pub const LATEST: i64;   // current schema version
```

### What ships when

Steps 1–4 (§6) move `Brain`, `VaultStore`, and `Mesh` into core **unchanged**, with
their existing method sets, so the app's ~60 call sites do not move. The free-function
surface above lands in step 5 as a facade over the same code, with `Brain`/`VaultStore`/
`Mesh` kept as `#[deprecated]` re-exports for one release. Do not ship a `0.1.0` of
`synapsecore` to an embedder before step 5 unless you are willing to break them.

## 6. `crates/synapsecore/Cargo.toml`

```toml
# Its own workspace root, for the same reason synapseserve is: Cargo loads every
# workspace member's manifest whatever you asked it to build, so as a member of
# the root workspace this crate could not be resolved without ../guise on disk —
# and a git or path dependency from nora, tandem, vibe, or merlin would fail on
# a machine that has no GPUI theming library checked out. Standing on its own is
# the whole point of the extraction.
#
# The cost is the same cost synapseserve pays: dependency versions are written
# out here instead of inherited, it keeps its own lockfile, and a root
# `cargo test` does not reach these tests. CI runs them in their own job, on
# Linux, with no guise checked out.
[workspace]

[package]
name = "synapsecore"
version = "0.1.0"
edition = "2024"
description = "Local memory engine for developer tools: SQLite storage, scopes, and recall budgets"

[dependencies]
anyhow = "1.0"
fs2 = "0.4"
serde = { version = "1.0", features = ["derive"] }
sha2 = "0.10"
sqlx = { version = "0.8", default-features = false, features = [
    "macros",       # FromRow / sqlx::Type derives only. No query!/query_as! —
                    # a compile-time-checked query would make this crate need a
                    # database at build time, and it deliberately has no path.
    "runtime-tokio",
    "sqlite",
] }

# .synapse.yaml only.
serde-saphyr = { version = "1.0.0-rc.1", optional = true }
# meshworker.arguments, stored as a JSON array.
serde_json = { version = "1.0", optional = true }
# JsonSchema derives for embedders that expose these types over MCP.
schemars = { version = "1.0", optional = true }
# The parked wait loop. No "process" feature: this crate starts nothing. No
# "rt-multi-thread": the embedder brings its own runtime.
tokio = { version = "1.0", features = ["time"], optional = true }

[features]
default = ["vault", "imports"]
vault   = ["dep:serde-saphyr"]
imports = []
mesh    = ["dep:tokio", "dep:serde_json"]
schema  = ["dep:schemars"]

[dev-dependencies]
tempfile = "3.20"
tokio = { version = "1.0", features = ["macros", "rt-multi-thread", "time"] }
```

Six dependencies with the defaults on; nine with everything. No gpui, no guise, no rmcp,
no security-framework, no directories, no rpassword, no libc, no chrono, no toml.

### Root workspace layout after the extraction

```
Cargo.toml                     [workspace] members = ["crates/synapse", "crates/synapsesync"]
                               exclude = ["crates/synapsecore", "crates/synapseserve"]
Cargo.lock
crates/
  synapse/        app: GPUI desktop + CLI + MCP. depends on synapsecore { path = "../synapsecore" }
  synapsecore/    own workspace root. own Cargo.lock. no guise, no gpui, no rmcp, no keychain.
  synapsesync/    wire format. member. unchanged.
  synapseserve/   own workspace root. own Cargo.lock. unchanged.
docs/synapsecore.md
```

`crates/synapse/Cargo.toml` gains one line:

```toml
synapsecore = { path = "../synapsecore", features = ["vault", "imports", "mesh", "schema"] }
```

and later sheds `fs2` (moves to core). It keeps `sqlx` and `tokio` because `mcp/`,
`cli/`, `relay/worker.rs`, and `skill/receipts.rs` still use them directly.

**Alternative I considered and rejected:** keep `synapsecore` as a member of the root
workspace and publish it to crates.io, so embedders take a version rather than a path or
git dep. That would sidestep the workspace-loading problem entirely and keep
`.workspace = true` inheritance. I rejected it because nora, tandem, vibe, and merlin all
want a path or git dependency *today*, and because publishing adds a second release
surface to a repo whose release trigger is currently one watched file. If Synapse ever
publishes to crates.io, revisit — membership becomes strictly better at that point.

## 7. Migration order

Each step compiles on its own and leaves `cargo test` green in both the root workspace
and `crates/synapsecore`. Do not bump `crates/synapse/Cargo.toml`'s `version` during any
of them (§8.3).

### Step 0 — skeleton and CI

Create `crates/synapsecore` with the manifest above and an empty `lib.rs`. Add
`"crates/synapsecore"` to the root `exclude` list. Add a CI job mirroring the existing
`server` job:

```yaml
  core:
    name: synapsecore · linux, no guise
    runs-on: ubuntu-latest
    defaults: { run: { working-directory: crates/synapsecore } }
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: crates/synapsecore }
      - run: cargo build --locked
      - run: cargo test --locked
```

Commit both lockfiles. Green trivially, and from this point the "builds without guise"
property is enforced rather than hoped for.

### Step 1 — `database`

Move `database.rs` and `database/{backup,lifecycle,migration,permission,tests}.rs`
verbatim. Replace `crates/synapse/src/database.rs` with a facade:

```rust
pub use synapsecore::database::{Opened, backupfolder, check, export, glance, open,
                                restore, securefile, snapshots};
```

Nothing else in the app changes — `crate::database::open` still resolves. `fs2` moves
from the app manifest to core's.

Green because `database/tests.rs` moves with the code and its `orphan()` helper still
finds the `secret` table (core owns migration 1).

### Step 2 — `memory` (from `brain`) and the import model

Move `brain/{store,optimize,settings,scope,ingest}.rs`, the engine half of
`brain/model.rs`, `imports/model.rs`, and `imports/secret.rs` into
`synapsecore::memory` and `synapsecore::imports`.

Surgeries, precisely:

- `brain/scope.rs:25`: `crate::vault::CONFIG` → `crate::CONFIG` (the const moves to
  core's `lib.rs` in this step, ahead of the rest of vault; the app's `vault.rs` re-exports
  it so `cli/shell.rs`, `cli/command.rs`, and `ui/dashboard.rs` keep compiling).
- `brain/model.rs`: every `#[derive(… JsonSchema …)]` becomes
  `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]`.
- `RememberRequest`/`RememberResponse`/`RecallRequest`/`RecallResponse` move to
  `crates/synapse/src/mcp/model.rs`, keeping their doc comments verbatim — those strings
  are the model-facing tool schema and `tests/mcp.rs` asserts on the surrounding wording.
- `imports/secret.rs`: `pub(crate) use secret::warning` → `pub use`.

`crates/synapse/src/brain.rs` becomes a facade: `pub use synapsecore::memory::{Brain,
Memory, MemoryScope, Optimization, Settings, Stats, projectroot};`. The rename
brain → memory is absorbed here at zero call-site cost.

Green: `brain/store.rs`'s eight `#[tokio::test]`s and `brain/ingest.rs`'s two move to
core and run under `cd crates/synapsecore && cargo test`. The root suite loses them —
this is the moment CLAUDE.md's "run both before calling anything green" becomes "run
three".

### Step 3 — the vault's store and scope halves

Move `vault/store.rs`, `vault/scope.rs`, and the data half of `vault/model.rs`. Add
`synapsecore::vault::{names, values}` with the warning check folded in. Rewrite
`vault/run.rs::environment` and `vault/shell.rs::changes` to call them:

```rust
// crates/synapse/src/vault/run.rs
pub async fn environment(folder: &Path) -> Result<Vec<(String, String)>> {
    let store = synapsecore::open(&crate::files::database()?).await?;
    let resolved = synapsecore::vault::resolve(&store, folder).await?;
    synapsecore::vault::values(&resolved, |account| getsecret(account))
}
```

`VaultStatusRequest`/`VaultStatusResponse`/`VaultScopeResponse` and the
`From<ScopeState>` impl move to `crates/synapse/src/mcp/model.rs`.

Green: `vault/store.rs`'s and `vault/scope.rs`'s tests move to core.
`yaml_scopes_override_global_and_require_approval` is the one to watch — it is the trust
and precedence contract in one test.

### Step 4 — the mesh's database half *(riskiest)*

Move `relay/{store,model,bus}.rs` to `synapsecore::mesh` behind `feature = "mesh"`, which
`crates/synapse` enables. `store::validname` becomes `pub`. `relay.rs` becomes a facade
over `synapsecore::mesh::*` plus the app's remaining submodules (`launch`, `harness`,
`worker`, `process`, `role`, `team`, `layer`).

**Why this is the riskiest step:**

1. It cuts a module in the middle of a hot loop. `relay/bus.rs`'s park is a 750ms tick
   with a 15s heartbeat and a hard `PARKSECONDS = 240` deadline, and it carries two
   `const _: () = assert!` blocks (`PARKSECONDS < CLIENTIDLEFLOOR`, and at least four
   progress reports per park). Those asserts must land in core with the constants, or the
   invariant that keeps a client from turning an abandoned park into an *error* — which
   makes an agent stop looping — is silently unenforced.
2. `tests/mcp.rs` runs **two `synapse mcp` processes against one tempdir** to exercise
   delivery across a real process boundary. It is the only test that would catch a
   cursor/placeholder regression introduced by the move (`placeholder` seeds the cursor
   from `tip()` captured *before* the triggering message is written), and it is
   process-spawning and timing-sensitive, so a failure here is slow to bisect.
3. It is where the feature-gating can go wrong quietly. If `crates/synapse` forgets
   `features = ["mesh"]`, `mcp/mesh.rs` fails to compile — loud. But if the `mesh` feature
   accidentally pulls `tokio`'s `process` or `rt-multi-thread` features, core silently
   grows a runtime it should not have, and nothing fails.
4. It is the only step where a type crosses the boundary *inside* an app-side loop:
   `relay/worker.rs` holds `Mesh` and `WorkerView` while supervising children, and
   `mcp/mesh.rs` holds `Mesh` alongside `Supervisor` in one `#[tool_router]`.

Second-riskiest is step 3, for a different reason: it is where the line between "metadata"
and "value" is drawn in code. A mistake there is a product-level invariant violation
("secret values never reach SQLite, YAML, MCP responses, or logs"), not a compile error.
Review that diff for what crosses into core, not for whether it builds.

### Step 5 — the free-function facade (optional, do last)

Add `Store`, `open`, `glance`, and the free functions of §5 over the existing internals.
Keep `Brain`/`VaultStore`/`Mesh` as `#[deprecated]` re-exports for one release, then
collapse the four opens of `brain.db` into one `Store` — which is the change that makes
the `VERIFIED` memo in `database.rs:81` mostly unnecessary and removes three file-lock
acquisitions per command.

Do not start step 5 until an embedder actually exists; the four-handle shape works and
the collapse touches `cli/session.rs`'s statusline path, which runs on every turn of
every session.

### Effect on the two-workspace split and CI

- The repo goes from **two workspaces to three**. CLAUDE.md's Build section needs
  rewriting: `cargo test` at the root, `cd crates/synapsecore && cargo test`, and
  `cd crates/synapseserve && cargo test` — three commands before anything is green, and
  after step 2 the *majority* of the memory tests live in the second one. This is the
  biggest ergonomic cost of the plan and it should be written down, not discovered.
- `.github/workflows/ci.yml` gains the `core` job above. The existing `check` job on
  macOS still builds the app (and therefore core, through the path dep) with guise
  checked out; the new job is what proves core stands alone. It is the exact analogue of
  the comment at ci.yml:95–99 for the server.
- `.github/workflows/release.yml`'s `test` job builds the whole app on macOS and is
  unaffected. Consider whether it should also run core's suite — I would say yes, one
  extra `cd crates/synapsecore && cargo test --locked` line, since the release gate is
  supposed to mean "green".
- `cargo fmt` at the root still must not use `--all`. It also will no longer reach
  `crates/synapsecore` (it is not a member) — add a second `cargo fmt --check` in the
  core directory to the advisory `fmt` job, which conveniently needs no guise checkout.
- `scripts/release:14` does `cd "$root/crates/synapse"` and is unaffected.

## 8. Risks and what will break

### 8.1 Test hermeticity

`crates/synapse/src/files.rs:15` defines `scopeddata`, a `#[cfg(test)]` process-wide
mutex around `unsafe { std::env::set_var("SYNAPSE_DATA", path) }`, used by
`relay/worker.rs:335` and `skill/install.rs:152` with `expect_used`-style allow comments
pointing at it. Both users stay in the app, so the guard must stay in the app's
`files.rs` — if `files/index.rs` is split and the guard follows the wrong half, those two
tests lose their serialization and start flaking against each other.

The upside is real too: core's tests already pass explicit tempdir paths
(`Brain::open(directory.path().join("brain.db"))`), so after the move core is hermetic by
construction — no env var can reach it. Lock that in with the `env::var` grep from §4.4.

`tests/cli.rs` and `tests/mcp.rs` set `SYNAPSE_HOME`, `SYNAPSE_DATA`, and `SYNAPSE_BIN`
on the spawned binary; they are unaffected because `files/index.rs` does not move.

### 8.2 `tests/mcp.rs`

Two `synapse mcp` children against one tempdir. It survives the split as long as (a) the
binary target keeps the name `synapse` (`env!("CARGO_BIN_EXE_synapse")`), (b)
`instructions::ensure` and `SOUL.md` stay app-side so the wording assertions
(`"At the start of every session"`, `"the `lean` budget first"`, `"instead of ad hoc
memory Markdown files"`) still hold, and (c) step 4 does not change delivery semantics.
CLAUDE.md already says changing that wording means updating this test deliberately — the
extraction is not a licence to touch it.

### 8.3 The release trigger

`.github/workflows/release.yml` fires on `push` to `main` with
`paths: ["crates/synapse/Cargo.toml"]`. **Every step of this migration that adds or
removes a dependency edits that file**, so every step will start the Release workflow.
It is safe *only* because `check-version` compares the version against
`git ls-remote --tags origin refs/tags/v${VERSION}` and prints "already released —
skipping" when it matches. Two consequences:

- Do not bump `version` in the same commit as a migration step. The standing "patch bump
  on every change" habit would ship a signed, notarized beta from a half-migrated tree.
- Changes under `crates/synapsecore/**` alone will **not** trigger a release. That is
  probably correct — core is a library, the release artifact is a Mac app — but it means a
  core fix reaches users only on the next app-manifest change. Decide deliberately
  whether to add `crates/synapsecore/Cargo.toml` to the watched paths; I would not, and
  would instead treat "bump the app when core changes" as the rule, matching the comment
  already at release.yml:6–9.

### 8.4 sqlx offline and macros

Verified: there are **zero** uses of `sqlx::query!`, `query_as!`, `query_scalar!`, or
`sqlx::migrate!` anywhere in the repo, and no `.sqlx` directory exists. Every query is a
runtime string. The `macros` feature is enabled only for `#[derive(FromRow)]`
(`Memory`, `Vault`, `Secret`, `Message`, `ImportBatch`) and `#[derive(sqlx::Type)]`
(`MemoryScope`, `MessageKind`). So the split creates **no** `DATABASE_URL` / `SQLX_OFFLINE`
requirement.

The rule to write down: **core must never introduce a compile-time-checked query.** Core
has no fixed database path by design (§4.4); a `query!` would require one at build time
and would undo the whole point.

Secondary hazard: because `synapsecore` is outside the root workspace, its `sqlx = "0.8"`
and the root's `[workspace.dependencies] sqlx = "0.8"` are two independent declarations.
If they ever drift across a semver-incompatible boundary, the graph gets two `sqlx`
crates and `SqlitePool` stops being the same type across the boundary — which matters,
because `crates/synapse` will hold `synapsecore::database::Opened { pool: SqlitePool }`.
Mitigation: core does `pub use sqlx;` and the app refers to `synapsecore::sqlx::…` where
it needs the type by name. `synapseserve` already lives with this duplication (its
manifest comment at lines 9–12 acknowledges it) but it never shares a type with the app,
so it never felt it.

### 8.5 FTS5 and the query shapes

Three performance-critical facts live in `brain/store.rs` as comments, and only two of
them have a test:

- `source` and `created` are `UNINDEXED` in the FTS5 table, so `created` needs
  `CAST(memory.created AS INTEGER)` on read. That cast appears in five query strings.
  No test asserts the cast; a hand-edit during the move that drops it would return the
  column as text and only some readers would notice.
- The recent-list query must select ids from the `memorymetacreated` index in a subquery
  *before* joining `memory`, or the planner drives from the full-text table (46ms vs
  0.2ms at 200k). Guarded only by
  `the_recent_list_orders_by_when_a_memory_was_made_not_when_it_was_written`, which
  checks *ordering*, not plan. **There is no performance regression test.** Moving the
  file is safe; rewriting the query during the move is not.
- `STOPWORDS` must not gain `not`, `no`, `never`, `always`, `use`, `run`, `all`. This one
  *is* tested (`the_expression_keeps_words_a_developer_might_have_meant`), and the test
  moves with the code.

**Unverified:** whether the SQLite that `sqlx 0.8` bundles on Linux
(`libsqlite3-sys 0.30.1` in `Cargo.lock`) has FTS5 compiled in. Migration 1's
`CREATE VIRTUAL TABLE … USING fts5(…)` fails at open time if it does not. I believe the
`bundled` build sets `SQLITE_ENABLE_FTS5`, but I did not build to confirm — **make the
first Linux run of step 2's tests the check**, because a failure there invalidates the
premise of the whole extraction.

### 8.6 Everything else worth naming

- **`cargo build --locked`.** CI uses it in three jobs. Adding a crate and a dependency
  changes the root `Cargo.lock` and creates `crates/synapsecore/Cargo.lock`; both must be
  committed with the step that changes them or CI fails before it compiles anything.
- **Feature drift.** If `crates/synapse` omits `features = ["schema"]`, the `JsonSchema`
  derives on `Memory` vanish and `mcp/model.rs`'s `RecallResponse` (which contains
  `Vec<Memory>`) fails to derive. That is a compile error, so it is a good failure — but
  worth stating in core's README so an embedder is not surprised.
- **A third `target/`.** `.gitignore` has `**/target/` (line 8), so no new entry is
  needed, but disk and CI cache size grow.
- **`schemars` is in the app's manifest for six other files** (`skill/model.rs`,
  `skill/install.rs`, `mcp/model.rs`, and the request/response types). It stays there; the
  core feature is additive, not a move.
- **`synapsesync` is not currently a dependency of `crates/synapse`.** If a sync client is
  ever built, it belongs in `synapsecore` (it needs the pool and the memory identity), and
  the edge would be `synapsecore → synapsesync`, never the reverse — `synapsesync`'s
  manifest comment forbids sqlx there. Worth noting before someone wires it the other way.
- **The `VERIFIED` memo becomes per-crate.** `database.rs:81`'s
  `static VERIFIED: Mutex<Vec<(PathBuf, Identity)>>` stays one static per *process*, since
  the app will only link one copy of core. If two semver-incompatible `synapsecore`
  versions ever end up in one binary, there would be two memos and one extra whole-page
  scan per open — correctness intact, performance halved. Not a real risk today, but it is
  the kind of thing that gets discovered rather than predicted.

# The embedding surface

How something that is not the Synapse desktop app consumes Synapse memory.

This is a design document, not a description of what ships today. Everything in
"What exists" is real and cited; everything after it is proposed.

---

## 1. What exists, and what does not

Three facts set every constraint that follows.

**There is no daemon and no port.** Every connected tool spawns its own
`synapse mcp` process (`crates/synapse/src/mcp.rs:5`). The agent mesh bus is four
tables in the same `brain.db` — `meshagent`, `meshsub`, `meshmessage`,
`meshworker` (`crates/synapse/src/database/migration.rs:58-97`) — coordinated by
SQLite WAL plus a shared advisory file lock
(`crates/synapse/src/database/permission.rs:36`). A parked `wait` polls on a
750ms tick (`crates/synapse/src/relay/bus.rs:47`). `crates/synapse/tests/mcp.rs`
runs two real `synapse mcp` processes against one tempdir and proves it works
across a process boundary. Multi-process concurrency is solved by shared-file
access, not by a server.

**There is no library.** `crates/synapse/src/main.rs` declares every module
privately (`mod brain;`, `mod database;`, …) and `crates/synapse/Cargo.toml`
has no `[lib]` target. Nothing outside this repository can write
`use synapse::brain::Brain`. Worse, the package depends on `gpui = "0.2.2"` and
`guise-ui.workspace = true`, which is a path dependency on a sibling checkout at
`../guise/crates/guise`. Even if a `lib.rs` were added tomorrow, an embedder
would need a GPUI theming library on disk to link a memory store.

So the "natural" embedding mode — in-process Rust against the shared local
`brain.db` — is not merely untested. It is currently impossible. That is the
headline finding.

**The precedent for fixing it already exists in this repo, twice.**
`crates/synapsesync` was split out because "anything added here is added to both
halves." `crates/synapseserve` is its own workspace root because Cargo loads
every workspace member's manifest whatever you asked it to build, so a headless
Linux server would otherwise have needed `../guise`. The embedding surface is
the third instance of the same problem and takes the same shape.

---

## 2. The mode matrix

| | Mode | Who it serves | Status |
|---|---|---|---|
| **a** | in-process Rust, shared user `brain.db` | the desktop app, the CLI, `synapse mcp`, `nora` (Rust/gpui) | blocked: no lib target |
| **b** | in-process Rust, private/standalone database | a shipped app, a container, a CI run, tests | already proven, unexposed |
| **c** | out-of-process stdio MCP | model-facing clients (Claude Code, Codex) | ships today |
| **d** | non-Rust process | Bun/TS (`tandem`, `vibe`, `merlin`), Python, Swift | does not exist |
| **e** | remote / networked memory | — | **not a mode.** See below. |

### (a) In-process against the shared local brain.db

**Serves.** Anything Rust running as the user on the user's Mac that wants the
same memory the desktop app and the connected tools see. `nora` is the archetype:
a gpui app that already links guise, already runs as the user, and wants ambient
Synapse without spawning a subprocess to get it.

**Shape.** `Brain::open(files::database()?)` — exactly what
`crates/synapse/src/mcp.rs:8` does. The handle carries a `SqlitePool` (5
connections, WAL, 5s busy timeout — `crates/synapse/src/database.rs:114-126`) and
an `Arc<File>` holding a shared `flock`.

**Failure modes.** Section 3 in full. The short list: no lib target today; a
long-lived shared lock blocks `synapse data restore`; a pinned older core is
permanently locked out after the app migrates; concurrent writers serialize
behind one WAL writer with a 5-second window.

### (b) In-process against a private/standalone database

**Serves.** Three distinct customers that look the same from here:

- A shipped app that wants Synapse-shaped memory for *its* user, not the
  developer's memory. `merlin` builds and hosts apps for a non-technical person;
  those apps must never read the developer's global memory.
- A container or CI run that must leave the host brain untouched.
- Tests. This mode is not speculative — every unit test in
  `crates/synapse/src/brain/store.rs` already runs it:
  `Brain::open(directory.path().join("brain.db"))`.

**Shape.** Identical code, explicit path, and crucially: **no `SYNAPSE_DATA`
resolution, no vault, no mesh.** The Keychain is per-user, not per-app
(`crates/synapse/src/vault/keychain.rs`), so a private brain has no credential
half at all. The mesh tables exist because they are in the migration, but nobody
else opens that file, so nobody joins.

**Failure modes.** Essentially none structural — this is the safest mode and the
one an embedder should reach for by default. The one real hazard is *accidental*
mode (b): see break #3.

### (c) Out-of-process over stdio MCP

**Serves.** Model-facing clients, and only those. The three tools are `remember`,
`recall`, `vaultstatus` (`crates/synapse/src/mcp/server.rs:47,65,90`), plus
sixteen mesh tools merged in only when the mesh setting is on
(`crates/synapse/src/mcp/server.rs:32-34`).

**Failure modes as an embedding surface.**

- One process per consumer. Each holds a 5-connection pool, a shared flock, and
  pays at least one whole-file `quick_check` on first open
  (`crates/synapse/src/database.rs:149`).
- The responses are *model-shaped*. `recall` runs everything through
  `optimize::recall` — compacted, deduplicated, truncated to a character budget
  (`crates/synapse/src/brain/optimize.rs:4-26`). A program that wants the stored
  body verbatim cannot get it. That is correct for a model and wrong for a
  program.
- The tool surface has no `delete`, no `export`, no `list`, no `search` that
  bypasses the budget. Those live only on the CLI.
- `synapse mcp` always resolves `files::database()`
  (`crates/synapse/src/mcp.rs:6`). There is no way to point it at a private
  brain, so mode (b) is unreachable over MCP.
- A Bun app that speaks MCP registers as a *tool*. If the mesh is on, it starts
  appearing where a roster expects a coding agent.

MCP stays exactly what it is: the model-facing contract, asserted by
`crates/synapse/tests/mcp.rs`. It is not the embedding surface.

### (d) A non-Rust process that needs memory

**Serves.** The first customers, all of them Bun: `tandem` (MCP tool fabric),
`vibe` (always-on Claude Code box), `merlin` (app builder). Plus anything else in
`~/Desktop/Dev` that is not Rust.

**Shape.** A C ABI over mode (a) or (b), loaded into the host process — not a
subprocess, not a socket. Section 8.

**Failure modes.** Everything from (a) or (b), plus: the ABI must own a tokio
runtime because sqlx is async; FFI string ownership must be explicit; and a
synchronous FFI call blocks the host's event loop, which rules out the mesh park
(Section 8).

### (e) Remote / networked memory is not a mode

Worth naming so it stops being proposed. `crates/synapseserve` is an append-only
log of sealed envelopes. It "cannot read a memory, cannot tell a stored one from
a deleted one, and resolves no conflicts" — its `/push` and `/pull` handlers
(`crates/synapseserve/src/http.rs:33,58`) move opaque bytes. There is no
server-side query. A machine that wants recall gets it by syncing to a local
brain and recalling locally. Cross-machine memory is a sync problem, never a
transport problem.

---

## 3. Where shared-file in-process access breaks

Seven real breaks, in rough order of how soon they bite.

### 3.1 There is no library target (blocker)

Covered above. The fix is Section 5.

### 3.2 Write concurrency under WAL

WAL gives many concurrent readers and exactly one writer. `connect()` sets
`busy_timeout(Duration::from_secs(5))` and `max_connections(5)`
(`crates/synapse/src/database.rs:119-123`). A single `remember` is a transaction
with two inserts (`crates/synapse/src/brain/store.rs:51-69`), and the FTS5 insert
writes shadow index tables as well.

At human rates this is invisible. It breaks when an embedder writes in a loop,
and the mesh already amplifies the write rate before any embedder shows up:

- `Mesh::insert` calls `prune()` on **every** message
  (`crates/synapse/src/relay/store.rs:168`), which can `UPDATE meshagent` and
  `DELETE FROM meshmessage`.
- Every parked `wait` calls `touch` every 15 seconds
  (`HEARTBEAT`, `crates/synapse/src/relay/bus.rs:52`), which is an `UPDATE`.
- Every mesh tool call touches.

Concrete break: a `vibe`-style box with eight parked agents plus a Bun importer
writing a few hundred memories. The importer's 5-second window is contended and
`remember` starts returning `database is locked` — which today surfaces to an
embedder as an untyped `anyhow` string it cannot branch on.

**This does not force a daemon.** It forces three things the design must carry:
a batch write (`rememberall`, one transaction), a longer busy timeout for
embedded handles, and `Failure::Busy` as a distinct, retryable error class
(Section 11).

### 3.3 A container or remote machine with no local brain.db — and the silent-empty failure

`files::data()` resolves `SYNAPSE_DATA`, else
`BaseDirs::data_local_dir().join("synapse")` (`crates/synapse/src/files/index.rs:17-24`).
Inside a container that path exists and is empty. `permission::prepare` creates
the parent (`crates/synapse/src/database/permission.rs:6-20`) and `connect()`
sets `create_if_missing(true)`.

So a container asking for "the user's memory" does not fail. It gets a brand new
empty brain, migrates it to version 7, and answers every recall with nothing.
That is the worst available failure: a confident empty answer. Every invariant in
this repo about "never report a connection that is not there" points the same
direction.

**The embedding API must make "the user's brain" and "a brain I own" different
arguments**, and `Location::User` must refuse rather than create. That is a
genuinely new behavior — `open()` cannot be reused unchanged.

### 3.4 Apps that must not see the user's global memory

`searchscoped` always ORs global memory with the current project's:

```sql
AND (meta.scope = 'global' OR (meta.scope = 'project' AND meta.project = ?))
```

(`crates/synapse/src/brain/store.rs:136`, and the same rule in `reach` at :179).
There is no project-only read path, deliberately — "Recall always returns global
memory *plus* the current project's."

That is right for a coding agent and wrong for a shipped app. An app embedding in
mode (a) reads the developer's global memory wholesale, including everything
`SOUL.md`-adjacent they ever stored as a preference.

**The answer is mode (b), not a filter.** Adding a `globals: false` flag to
recall would make global memory feel optional to every caller, which is exactly
the property the scope rule exists to prevent. An app that must not see the
user's memory gets its own database file.

### 3.5 The once-per-process `quick_check` memoization

`VERIFIED` is a process-global `Mutex<Vec<(PathBuf, Identity)>>`
(`crates/synapse/src/database.rs:81`) keyed on the size and mtime of the database
*and* its `-wal` sidecar (`identity()`, :96). It exists because one CLI command
opens the store three or four times.

In a long-lived embedder the cost profile inverts. `unverified()` compares the
memo against the file's *current* state, so any write by any other process
invalidates it. In a busy multi-writer setup — which is precisely what embedding
creates — an embedder that opens a handle per request pays a whole-file page scan
on nearly every request, and the cost grows with the store.

Two consequences for the design:

- A long-lived embedder opens **once** and holds the handle. The SDK must make
  that the obvious thing and per-call opens the awkward thing.
- A reporting embedder uses `glance` (`crates/synapse/src/database.rs:41`), which
  skips the scan and never records a memo — the same reasoning that made
  `statusline` use it (`crates/synapse/src/cli/session.rs:216`).

One trap worth writing down: **`glance` is not a read-only mode.** `opened()`
runs `migration::run` regardless of the scan setting
(`crates/synapse/src/database.rs:66`). A truly read-only open is
`database::readonly()` (:128), which is currently private, takes no lock, and
runs no migration. Mode (b)-for-inspection needs that one exposed.

### 3.6 Backups and migrations racing

Three separate hazards under one heading.

**Migration on every open.** `migration::run` executes on every `open()` and
every `glance()`. The first process to open after an upgrade migrates, taking a
pre-migration backup first (`crates/synapse/src/database/migration.rs:160-164`)
and running all statements in one transaction. Other processes holding pools at
that moment block on the write lock — within the 5-second busy timeout for small
migrations, and not necessarily for a large one like migration 5, which rewrites
`memorymeta.created` across the whole store.

**A pinned older embedder is permanently locked out.** `migration::run` refuses
when `current > LATEST`:

```rust
anyhow::ensure!(
    current <= LATEST,
    "database version {current} is newer than this Synapse release supports"
);
```

(`crates/synapse/src/database/migration.rs:152-155`, `LATEST: i64 = 7` at :6.)
Correct for the app. Fatal for embedding: a Bun app pinned to an SDK built
against schema 7 stops working the moment the user updates Synapse to schema 8,
and there is no way back because the migration is not reversible. This is the
strongest argument in the document for an explicit schema-compatibility contract
(Section 12).

**A long-lived handle makes restore impossible.** `lifecycle::restore` takes
`permission::exclusivelock`, which is `try_lock_exclusive` — non-blocking, fails
immediately — and its error text is:

> "Synapse is using this database; close the app and connected tools before restoring"

(`crates/synapse/src/database/permission.rs:42-48`, called at
`crates/synapse/src/database/lifecycle.rs:52`.)

An embedded handle holds `sharedlock` for its entire life. A Bun server that
never exits therefore makes `synapse data restore` permanently fail, and the
error tells the user to close things they have no idea are running. **The
embedding surface creates this product bug**, and the design has to answer it:
the SDK must expose `close`, must document that a held handle blocks restore, and
the error message should grow to name what is holding the lock.

### 3.7 The Keychain is reachable from inside the crate

`vault::getsecret(account) -> String` is a free function in
`crates/synapse/src/vault.rs:24`. If an extracted crate exported the vault module,
the embedding surface would become a credential-read API in one `pub use`. The
extraction must leave `vault` behind entirely. Section 10.

---

## 4. Does a local daemon ever get built?

**No. Recommendation: never build a memory daemon.**

The concurrency problem a daemon would solve is already solved. WAL plus
`sharedlock` handles many processes; `crates/synapse/tests/mcp.rs` demonstrates
two real `synapse mcp` processes coordinating over one file. A daemon would add a
socket to secure, a lifecycle to supervise, version skew between daemon and
client, and a single point of failure for a product whose stated position is that
a server outage "is not an emergency" (`CLAUDE.md`, line 9). It would also
contradict the relay design directly: "there is no daemon, port, or bearer token"
(`CLAUDE.md`, relay section).

**The one thing that would force a daemon is a wake signal** — genuinely
sub-second, poll-free message delivery on the mesh. The design has already
declined that trade deliberately: `PARKSECONDS = 240`, `TICK = 750ms`, and a
const assert tying `PARKSECONDS` under `CLIENTIDLEFLOOR`
(`crates/synapse/src/relay/bus.rs:26-44`). One indexed query per agent per tick
is cheaper than a daemon, and the mesh's consumer is a model's tool-call loop,
which does not notice 750ms.

**Write contention (3.2) does not force one either.** The fix is a batch write
plus a longer busy timeout plus a typed `Busy` error, all of which are cheaper
and more honest than serializing every write through a process that can crash.

**Named exception, stated exactly.** X = *an embedder running on a machine that
is not the user's Mac and that needs the user's memory* — `vibe` on krillin,
`tandem` workers on a remote host. The answer there is still not a memory daemon.
It is `synapseserve` sync plus a local brain on that machine, because that is the
only shape that keeps the end-to-end-encryption property: the server never reads a
memory. Write it as a sync target, not a daemon.

---

## 5. The extraction: `crates/synapsecore`

Everything below depends on one structural change. It is small and the dependency
graph says so.

`crates/synapse/src/database/` has **zero** cross-module dependencies — nothing in
it references `crate::brain`, `crate::vault`, or anything else. `crates/synapse/src/brain/`
has exactly two:

- `brain/scope.rs:25` uses `crate::vault::CONFIG` — a single `&'static str`
  constant, `".synapse.yaml"` (`crates/synapse/src/vault/scope.rs:8`).
- `brain/ingest.rs:2` uses `crate::imports::{…}` — an `impl Brain` block carrying
  the previewed-import machinery.

So:

```
crates/synapsecore/           # new; no gpui, no guise, no rmcp, no keychain
  src/
    lib.rs                    # pub use of the surface in Section 6
    database.rs               # moved verbatim
    database/{backup,lifecycle,migration,permission}.rs
    brain.rs
    brain/{model,optimize,scope,settings,store}.rs
    handle.rs                 # new: Location, Failure, open/glance/close
```

Two adjustments the move requires:

1. `CONFIG` moves into `synapsecore` (it names the file `projectroot` looks for),
   and `crate::vault::scope` re-exports it so nothing else changes.
2. `brain/ingest.rs` is an inherent `impl Brain`, so it cannot stay in the app
   crate once `Brain` moves — the orphan rule permits inherent impls only in the
   defining crate. Split it the way the conventions already prefer: the SQL-level
   half (`importbatch` / `memoryorigin` rows) becomes free functions in
   `synapsecore` taking `&Brain` plus plain data, and the provider parsers in
   `imports/{claude,codex,markdown,secret}.rs` stay in the app crate and produce
   that data. "Prefer data and free functions over class-like abstractions" —
   this is that rule paying off.

`crates/synapse` then depends on `synapsecore` and keeps `ui`, `cli`, `mcp`,
`relay`, `vault`, `agent`, `skill`, `imports`, `instructions`, `crashes`, `files`.

It versions independently, like `synapseserve`. This matters concretely:
**bumping `version` in `crates/synapse/Cargo.toml` on `main` is the release
trigger** for the signed and notarized Mac build (`CLAUDE.md`, line 65). An SDK
patch must never drag that pipeline along.

Workspace placement: `synapsecore` is a member of the **root** workspace
alongside `crates/synapse` and `crates/synapsesync`. It has no guise dependency of
its own, but the root workspace already cannot build without `../guise` on disk,
so a consumer outside this repo depends on the published crate, not the path.

---

## 6. The API surface

Conventions: lowercase identifiers with no underscores, `CamelCase` types, data
and free functions over class-like abstractions, `anyhow`-style lowercase prose in
messages.

### Opening

```rust
/// Which brain, and on what terms.
pub enum Location {
    /// The person's own brain, at `SYNAPSE_DATA`/`brain.db` or the platform
    /// data directory. Refuses when it is not there — never creates one, so a
    /// container asking for memory that is not on this machine hears about it
    /// instead of getting an empty store that answers every question with
    /// nothing.
    User,
    /// A brain this program owns. Created on first open. No `SYNAPSE_DATA`, no
    /// vault, no shared roster.
    Private(PathBuf),
    /// An existing file opened for reading. No lock, no migration, no writes.
    Readonly(PathBuf),
}

/// How much verification an open pays for.
pub enum Care {
    /// Read every page before trusting the store. Costs time in proportion to
    /// everything stored, memoized per process on file size and mtime.
    Whole,
    /// Relationship check only. For a handle that reports rather than decides.
    Glance,
}

/// A live connection. Cheap to clone, holds the pool and the shared lock.
pub struct Handle { /* private */ }

pub async fn open(location: Location, care: Care) -> Result<Handle, Failure>;

/// Release the pool and the shared lock. A held handle blocks
/// `synapse data restore`, so a long-lived embedder that goes quiet should let
/// go of it.
pub async fn close(handle: Handle);

/// What this handle is attached to, for a status line or a bug report.
pub fn at(handle: &Handle) -> &Path;
```

`open(Location::User, _)` performs the existence check `permission::prepare`
currently only *reports* (`existed = path.is_file() && len > 0`) and turns a
missing file into `Failure::Nobrain` rather than a fresh empty database.

### Remembering

```rust
pub struct Remembrance {
    /// Durable fact, decision, preference, or correction.
    pub content: String,
    /// Where it came from: a project path, a topic, a tool name.
    pub source: Option<String>,
    /// Project by default; global only for something useful everywhere.
    pub scope: MemoryScope,
    /// Absolute project root. Resolved through `projectroot`, so any path
    /// inside the project works.
    pub project: Option<PathBuf>,
}

pub async fn remember(handle: &Handle, entry: &Remembrance) -> Result<i64, Failure>;

/// One transaction for the lot. The answer to 3.2: a caller writing a batch
/// takes the writer lock once instead of once per memory.
pub async fn rememberall(handle: &Handle, entries: &[Remembrance]) -> Result<Vec<i64>, Failure>;
```

Both funnel to `Brain::rememberscoped` (`crates/synapse/src/brain/store.rs:36`),
which trims, refuses empty content, and resolves the project through
`MemoryScope::project` (`crates/synapse/src/brain/model.rs:36`).

### Recalling

```rust
pub struct Ask {
    /// Words or a phrase. Stopwords are dropped; a query left with nothing
    /// routes to the recent list rather than matching on `are`.
    pub query: String,
    /// Requested match count. The configured optimization lowers it.
    pub limit: u32,
    /// Per-call budget. Can shrink the configured budget, never grow it.
    pub budget: Option<Optimization>,
    /// Absolute project root. Global memory plus this project's is returned;
    /// other projects stay out.
    pub project: Option<PathBuf>,
}

pub struct Answer {
    /// What was actually applied after `constrained`. Not what was asked for.
    pub optimization: Optimization,
    pub memories: Vec<Memory>,
}

pub async fn recall(handle: &Handle, ask: &Ask) -> Result<Answer, Failure>;
```

An empty result is `Ok(Answer { memories: vec![] })`. Never a `Failure`. This is
already the semantics —
`a_query_never_matches_on_its_function_words_alone`
(`crates/synapse/src/brain/store.rs:415`) asserts that returning nothing is the
*correct* answer to a bad query, because "an agent acts on a confident wrong
answer, so nothing is the better result."

### Status, scope, delete, export

```rust
pub struct Standing {
    /// How many memories a recall from this project could draw on: everything
    /// global plus everything for that project. The same scope rule recall
    /// uses, so the number reported is the number reachable.
    pub reach: i64,
    pub project: Option<String>,
    /// The user's configured budget, before any per-call reduction.
    pub optimization: Optimization,
    /// `PRAGMA user_version`. What an embedder checks against its own support
    /// range before it trusts anything else here.
    pub schema: i64,
    pub entries: i64,
    pub bytes: u64,
}

pub async fn status(handle: &Handle, project: Option<&Path>) -> Result<Standing, Failure>;

/// The project a path belongs to: the closest ancestor carrying a `.git` or a
/// `.synapse.yaml`, or the folder itself. A path that does not resolve is
/// `Ok(None)` — the absence of a project, not an error.
pub fn projectroot(path: &Path) -> Result<Option<PathBuf>, Failure>;

/// Remove one memory and its origin rows. Returns what was removed, or `None`
/// when there was no such id.
pub async fn forget(handle: &Handle, id: i64) -> Result<Option<Memory>, Failure>;

/// A consistent vacuumed copy, owner-only, integrity-checked before it returns.
pub async fn export(handle: &Handle, target: &Path) -> Result<(), Failure>;
```

`status` composes `Brain::reach` (`store.rs:171`), `Brain::settings` (:295),
`Brain::stats` (:319) and `PRAGMA user_version` — the same set
`cli/session.rs::gather` assembles for the status line, minus the mesh and vault
halves.

`export` wraps `database::export` (`crates/synapse/src/database/lifecycle.rs:25`),
which refuses an existing target, vacuums, secures the file to 0600, and
re-verifies before returning.

### What is deliberately absent

- **`setoptimization`.** See Section 7. This is the actual hole.
- **`wipememories`.** Guarded destruction is a decision a person makes in the app
  or with `--confirm` on the CLI, not an API call a library can make on their
  behalf. "Memory is never removed as a side effect."
- **`restore`.** It needs an exclusive lock that a live embedder is, by
  definition, holding.
- **Everything vault.** Section 10.
- **Everything mesh.** Section 8.
- **`search` (unbudgeted).** `Brain::search` (`store.rs:94`) reads raw bodies past
  the optimization budget. It is what the desktop memory browser and
  `memory list` use. Exposing it to embedders would be a second read path that
  does not go through `constrained`, which is the one thing Section 7 exists to
  prevent.

---

## 7. Preserving `Optimization::constrained` across every transport

The invariant: a per-call budget may only **shrink** the user's configured
budget, never grow it.

```rust
pub fn constrained(self, requested: Option<Self>) -> Self {
    match (self, requested.unwrap_or(self)) {
        (Self::Lean, _) | (_, Self::Lean) => Self::Lean,
        (Self::Balanced, _) | (_, Self::Balanced) => Self::Balanced,
        (Self::Full, Self::Full) => Self::Full,
    }
}
```

(`crates/synapse/src/brain/model.rs:93-100` — a meet over the lattice
Full > Balanced > Lean.)

**The load-bearing structural fact: `constrained` is applied inside
`Brain::recallscoped`, not in the MCP layer.**

```rust
let configured = self.settings().await?;
let settings = Settings::from(configured.optimization.constrained(budget));
```

(`crates/synapse/src/brain/store.rs:80-81`.) `mcp/server.rs:68` merely forwards
`request.budget` through. So the invariant is not transport policy that each new
surface has to re-implement and can forget — it is a property of the one function
every read must go through.

Four rules keep it that way across the C ABI, the TS SDK, and anything later.

**Rule 1 — `recallscoped` is the only public read path.** `synapsecore::recall` is
a thin wrapper over it. `Brain::search` is not re-exported. Any future transport
that wants memory calls `recall` or does not get memory.

**Rule 2 — the budget type is `Option<Optimization>` everywhere, never
`Settings`.** `Settings` carries `resultlimit: u32` and
`characterbudget: Option<usize>` (`brain/model.rs:115-120`) and is reachable only
through `impl From<Optimization> for Settings` (:122). That conversion is
one-way and stays one-way. An embedder cannot construct
`Settings { characterbudget: Some(1_000_000), .. }` because `Settings` is never an
input to anything. Over the C ABI the field is the string `"full" | "balanced" |
"lean"`, parsed by `Optimization::from_str` (:102), which errors on anything else
— there is no numeric budget to inflate.

**Rule 3 — `limit` needs no validation because it is already re-clamped.**

```rust
self.searchscoped(query, limit.clamp(1, settings.resultlimit), &project)
```

(`store.rs:89`.) The clamp uses the *post-`constrained`* settings.
`cli/session.rs:249` relies on this deliberately, passing `u32::MAX` and receiving
the setting's ceiling. So the C ABI can accept a raw `uint32_t` and stay safe, and
the TS SDK does not need to bounds-check.

**Rule 4 — and this is the one that is easy to miss — `setoptimization` is not in
the embedding surface.**

`constrained` guards the per-call budget. It does not guard the *configured*
budget. An embedder that could call `setoptimization(Optimization::Full)` and then
`recall` has escaped the invariant in two calls without ever violating
`constrained`. `Brain::setoptimization` (`store.rs:299`) writes the `setting` row
that `settings::read` (`brain/settings.rs:6`) later reads back as the ceiling.

So: `synapsecore` exports `Optimization` and `Settings` as *outputs*, exports
`recall`, and does **not** export `setoptimization`, `setpreference`, or
`setmesh`. Configuring the ceiling stays with the person, through
`synapse settings optimize <full|balanced|lean>` or the desktop app. The app crate
reaches those through a `#[doc(hidden)] pub mod owner` module that is documented
as "the desktop app and the CLI only" — one module to audit rather than a rule
everyone has to remember.

**Reporting, not just enforcing.** `Answer.optimization` carries what was
*applied*, mirroring `RecallResponse.optimization` (`brain/model.rs:176`). An
embedder that asked for Full and is running under a user configured to Lean can
see that and say so, rather than silently believing it got everything. The same
pattern the session hook already documents:

```rust
/// The budget the session hook recalls under. A per-call budget can only shrink
/// the user's configured one, so this is a ceiling and not an override: someone
/// running Lean still gets Lean.
const BUDGET: Optimization = Optimization::Balanced;
```

(`crates/synapse/src/cli/session.rs:28-32`.)

---

## 8. The C ABI

`crates/synapseffi` — `crate-type = ["cdylib", "staticlib"]`, wrapping
`synapsecore` with an owned tokio current-thread runtime, because sqlx is async
and a C caller is not.

```c
/* synapse.h */

typedef struct synapsehandle synapsehandle;

/* 0 on success. Anything else is a code from section 11. */

int32_t synapseopen(int32_t location, const char *path, int32_t care,
                    synapsehandle **out);
int32_t synapseclose(synapsehandle *handle);

/* Request and response are both JSON, UTF-8, NUL-terminated. The caller owns
   *out and releases it with synapsefree. */
int32_t synapserecall  (synapsehandle *handle, const char *request, char **out);
int32_t synapseremember(synapsehandle *handle, const char *request, char **out);
int32_t synapsestatus  (synapsehandle *handle, const char *project, char **out);
int32_t synapseforget  (synapsehandle *handle, int64_t id, char **out);
int32_t synapseexport  (synapsehandle *handle, const char *target);

void synapsefree(char *value);

/* Thread-local, valid until the next call on this thread. Lowercase prose, the
   same text the Rust error carries. Never carries a secret or a memory body. */
const char *synapselasterror(void);

/* The ABI generation. Not the crate version. */
uint32_t synapseabi(void);
```

**Why JSON payloads rather than C structs.** The request and response shapes are
already `serde` + `schemars` types in `crates/synapse/src/brain/model.rs`
(`RememberRequest`, `RecallRequest`, `RecallResponse`, `Memory`). Reusing them
means the C ABI, the MCP tool schema, and the TS types cannot drift, and adding a
field is not an ABI break — the function count and signatures are what freeze, not
the payload. `synapsesync` makes the same choice for the same reason: `PROTOCOL`
is a number over a stable payload, "deliberately not derived from any crate
version."

**Rust side, one function, for shape:**

```rust
#[unsafe(no_mangle)]
pub extern "C" fn synapserecall(
    handle: *mut Handle,
    request: *const c_char,
    out: *mut *mut c_char,
) -> i32 {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return record(Failure::Broken(anyhow::anyhow!("no open brain")));
    };
    let Ok(request) = readtext(request) else {
        return record(Failure::Broken(anyhow::anyhow!("the request is not utf-8")));
    };
    let ask: Ask = match serde_json::from_str(&request) {
        Ok(ask) => ask,
        Err(error) => return record(Failure::Broken(error.into())),
    };
    match runtime().block_on(synapsecore::recall(handle, &ask)) {
        Ok(answer) => {
            unsafe { *out = intotext(serde_json::to_string(&answer).unwrap_or_default()) };
            0
        }
        Err(failure) => record(failure),
    }
}
```

Edition 2024, hence `#[unsafe(no_mangle)]`.

### The C ABI is memory-only. The mesh stays on MCP.

A `bun:ffi` call is synchronous — it blocks the JavaScript event loop until it
returns. Memory operations are SQLite-fast: `CLAUDE.md` records 0.2ms for a
scoped recent-list query and 0.3ms for a clean full-text search at 200k memories.
Blocking for that is fine.

A mesh `wait` parks for up to `PARKSECONDS = 240` seconds
(`crates/synapse/src/relay/bus.rs:26`). Exposing that over a synchronous FFI would
freeze a Bun process for four minutes. And the park's whole contract — return
empty rather than error, because "an agent that sees an error stops looping"
(:22-25), with progress notifications every 30 seconds so the client does not age
the call out — is written for a model's tool-call loop. It has no meaning for an
event loop.

So the line is clean and worth stating as a rule: **the C ABI carries memory. The
mesh is MCP-only.** A Bun program that genuinely wants to be on the mesh should
run `synapse mux` or spawn a real agent, not pretend to be one through FFI.

---

## 9. The Bun / TypeScript SDK

### Decision

Three candidates were on the table.

| | Approach | Verdict |
|---|---|---|
| 1 | bind the C ABI via `bun:ffi` | **recommended** |
| 2 | spawn `synapse mcp` and speak JSON-RPC | fallback only |
| 3 | local HTTP or unix-socket endpoint | **rejected** |

**Reject (3) outright.** A local endpoint is a daemon with a different name. It
needs a listener, a lifecycle, a supervisor, and a secret — and the relay section
of `CLAUDE.md` records that avoiding exactly those three things ("no daemon, port,
or bearer token") is what let the mesh be four tables instead of a service. Adding
one for the SDK would give Synapse a daemon by the side door after Section 4 said
no through the front.

**Keep (2) as a fallback, not the SDK.** It works today with zero new Rust, which
is worth something. But it costs a process per consumer; the responses come back
compacted, deduplicated, and truncated by `optimize::recall` so a program cannot
get a stored body verbatim; there is no `forget` or `export` in the tool surface;
`synapse mcp` always opens `files::database()` so `Location::Private` is
unreachable; and if the mesh is on, the Bun app appears on the agent roster as a
tool. Ship it as `fallback.ts`, used when the dylib is not on disk, and say so in
the type of the handle.

**Recommend (1).** `bun:ffi` `dlopen` into the host process is the same
shared-file in-process access every other Synapse process already performs. It
adds no new coordination mechanism, no new port, and no new lifecycle — the
handle's lifetime is the Bun process's lifetime, which is exactly what
`synapse mcp` already does. It is the only option that reaches mode (b), and the
only one that returns unbudgeted structure when a program asks for it.

The SDK functions are `async` even though the FFI calls are synchronous, so the
implementation can move to a worker thread later without an API break.

### Layout

Per the conventions — lowercase, no dashes or underscores, `dir/{index.ts, thing.ts}`,
small focused files, functional, no classes.

```
packages/synapse/
  index.ts       # re-exports, and nothing else
  types.ts       # Brain, Memory, Budget, Scope, Ask, Standing
  open.ts        # dlopen, open, close
  recall.ts
  remember.ts
  status.ts
  forget.ts
  errors.ts      # SynapseError and the code predicates
  fallback.ts    # spawn `synapse mcp` when the dylib is absent
```

### API shape

```ts
// packages/synapse/types.ts

export type Budget = "full" | "balanced" | "lean";
export type Scope = "global" | "project";

export type Memory = {
  readonly id: number;
  readonly body: string;
  readonly source: string;
  readonly scope: Scope;
  readonly project: string;
  /** Unix seconds. */
  readonly created: number;
};

/** Opaque. Data, not an object with methods. */
export type Brain = {
  readonly at: string;
  readonly via: "ffi" | "mcp";
  readonly handle: unknown;
};

export type Ask = {
  query: string;
  project?: string;
  limit?: number;
  /** Can shrink the user's configured budget. Never grows it. */
  budget?: Budget;
};

export type Answer = {
  /** What was applied, not what was asked for. */
  readonly optimization: Budget;
  readonly memories: readonly Memory[];
};

export type Standing = {
  readonly reach: number;
  readonly project: string | null;
  readonly optimization: Budget;
  readonly schema: number;
  readonly entries: number;
  readonly bytes: number;
};
```

```ts
// packages/synapse/index.ts

export { open, close } from "./open";
export { recall } from "./recall";
export { remember } from "./remember";
export { status } from "./status";
export { forget } from "./forget";
export { SynapseError, isbusy, isnobrain, isschema } from "./errors";
export type { Brain, Memory, Budget, Scope, Ask, Answer, Standing } from "./types";
```

Free functions over a data handle. No classes, no builder, no fluent chain.

```ts
export async function open(
  where: { user: true } | { private: string } | { readonly: string },
  care?: "whole" | "glance",
): Promise<Brain>;

export async function close(brain: Brain): Promise<void>;

export async function recall(brain: Brain, ask: Ask): Promise<Answer>;

export async function remember(brain: Brain, entry: {
  content: string;
  source?: string;
  scope?: Scope;
  project?: string;
}): Promise<{ id: number }>;

export async function status(brain: Brain, project?: string): Promise<Standing>;

export async function forget(brain: Brain, id: number): Promise<Memory | null>;
```

### The dlopen half

```ts
// packages/synapse/open.ts
import { dlopen, FFIType, suffix, CString, type Pointer } from "bun:ffi";
import type { Brain } from "./types";
import { SynapseError } from "./errors";
import { openviamcp } from "./fallback";

const USER = 0, PRIVATE = 1, READONLY = 2;
const WHOLE = 0, GLANCE = 1;

const symbols = {
  synapseopen:     { args: [FFIType.i32, FFIType.ptr, FFIType.i32, FFIType.ptr], returns: FFIType.i32 },
  synapseclose:    { args: [FFIType.ptr], returns: FFIType.i32 },
  synapserecall:   { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  synapseremember: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  synapsestatus:   { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  synapseforget:   { args: [FFIType.ptr, FFIType.i64, FFIType.ptr], returns: FFIType.i32 },
  synapsefree:     { args: [FFIType.ptr], returns: FFIType.void },
  synapselasterror:{ args: [], returns: FFIType.cstring },
  synapseabi:      { args: [], returns: FFIType.u32 },
} as const;

/** The ABI generation this SDK was written against. */
const ABI = 1;

let library: ReturnType<typeof dlopen<typeof symbols>> | null = null;

function load() {
  if (library) return library;
  const path = Bun.env.SYNAPSE_LIB ?? `libsynapse.${suffix}`;
  library = dlopen(path, symbols);
  const found = library.symbols.synapseabi();
  if (found !== ABI) {
    throw new SynapseError(
      5,
      `this sdk speaks synapse abi ${ABI} and the installed library speaks ${found}`,
    );
  }
  return library;
}

export function text(value: string) {
  return Buffer.from(`${value}\0`, "utf8");
}

/** Read and release an owned `char *` written into an out slot. */
export function take(slot: BigUint64Array): string {
  const pointer = Number(slot[0]) as Pointer;
  if (!pointer) return "";
  const value = new CString(pointer).toString();
  load().symbols.synapsefree(pointer);
  return value;
}

export function check(code: number): void {
  if (code === 0) return;
  throw new SynapseError(code, load().symbols.synapselasterror()?.toString() ?? "");
}

export async function open(
  where: { user: true } | { private: string } | { readonly: string },
  care: "whole" | "glance" = "whole",
): Promise<Brain> {
  let lib;
  try {
    lib = load();
  } catch (error) {
    if (error instanceof SynapseError) throw error;
    return openviamcp(where);
  }

  const [location, path] =
    "user" in where ? [USER, ""] :
    "private" in where ? [PRIVATE, where.private] :
    [READONLY, where.readonly];

  const slot = new BigUint64Array(1);
  check(lib.symbols.synapseopen(
    location,
    text(path),
    care === "glance" ? GLANCE : WHOLE,
    slot,
  ));
  return { at: path, via: "ffi", handle: Number(slot[0]) };
}

export async function close(brain: Brain): Promise<void> {
  if (brain.via !== "ffi") return;
  load().symbols.synapseclose(brain.handle as Pointer);
}
```

```ts
// packages/synapse/recall.ts
import type { Answer, Ask, Brain } from "./types";
import { check, take, text } from "./open";
import { recallviamcp } from "./fallback";
import { library } from "./open";

export async function recall(brain: Brain, ask: Ask): Promise<Answer> {
  if (brain.via === "mcp") return recallviamcp(brain, ask);
  const slot = new BigUint64Array(1);
  check(library().symbols.synapserecall(
    brain.handle as never,
    text(JSON.stringify({ limit: 8, ...ask })),
    slot,
  ));
  return JSON.parse(take(slot)) as Answer;
}
```

Note what is *not* there: no budget arithmetic, no limit clamping, no
optimization setter. The budget is a string that `Optimization::from_str`
validates, and the limit is re-clamped in `recallscoped`. The SDK cannot escape
the ceiling because it has nothing to escape it with.

---

## 10. Auth and safety

### The honest baseline

For the recommended transport there is no endpoint, so there is no authentication
surface to design. That is the strongest argument for it.

**What stops an arbitrary local process from reading the user's whole memory:
the filesystem, and nothing else.** `permission::securefile` sets 0600 on
`brain.db`, `brain.db-wal`, `brain.db-shm`, and `brain.lock`
(`crates/synapse/src/database/permission.rs:22-33,50`); `securedirectory` sets
0700 on the data directory (:60). Any process running as that user can open the
file with `sqlite3` and read every memory.

That is already true today. `synapse mcp` is wired into a tool by naming a binary
in a config file; nothing authenticates the tool. The embedding SDK does not
lower the bar. It must simply not raise a *new* one — and the one place it could
is the vault.

### The vault line

`vault::getsecret(account) -> String` (`crates/synapse/src/vault.rs:24`) reads
directly from Keychain. It is a free function reachable from anywhere in the
`synapse` crate.

**`crates/synapsecore` contains `database` and `brain`. It does not contain
`vault`, and cannot re-export it.** That is a one-line rule, verifiable by reading
the new crate's `Cargo.toml` and `lib.rs`, which is exactly why it is the right
shape for an invariant — "secret values never reach SQLite, `.synapse.yaml`, MCP
responses, or logs" becomes "secret values are not in the crate."

If credential *metadata* is ever wanted at the embedding layer, it goes in the app
crate behind a separate ABI entry point and copies the shape `vaultstatus` already
established:

```rust
/// Variable names only. Never a value, never a Keychain account reference.
pub async fn vaultnames(vaults: &VaultStore, path: &Path) -> Result<Vec<String>>;
```

Built from `Resolved.env.keys().cloned().collect()` — precisely what
`crates/synapse/src/mcp/server.rs:115` does for
`VaultStatusResponse.available`. Not from `Secret`. `Secret.account`
(`crates/synapse/src/vault/model.rs:19`) is the Keychain lookup key, and handing
that out is one `security find-generic-password` away from the value. The MCP
surface already gets this right; the embedding surface copies that shape and not
the struct.

The recommendation is to not ship `vaultnames` in the first version. A program
that wants credentials should use `synapse run -- <command>`, which refuses on any
scope warning and hands a fully resolved environment to exactly one child.

### Error text must not become a leak

`synapseserve` sets the precedent:

```rust
impl From<anyhow::Error> for Failure {
    /// Anything that reached here is this server's problem, not a description
    /// of it that a client should be handed.
    fn from(error: anyhow::Error) -> Self {
        eprintln!("synapseserve: {error:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "the server could not complete this request")
    }
}
```

(`crates/synapseserve/src/http.rs:146-156`.)

The embedding case is different — the caller is on the same machine as the user,
so the anyhow chain is useful and safe to return. `synapselasterror()` returns it
verbatim. But the rule that makes that safe has to be written down and kept:

> Nothing that came out of Keychain, and nothing that came out of a memory body,
> may reach an error string.

The codebase already holds to this. `rememberscoped` fails with "memory content
cannot be empty" and never echoes the content
(`crates/synapse/src/brain/store.rs:44`). `MemoryScope::project` fails with
"project-scoped memory needs a project path" (`brain/model.rs:44`). Paths appear;
content does not. `crates/synapseserve/src/auth.rs:20` goes further and gives
`Token` no `Debug` and no `Display` at all, because "a token in a log line is a
token in a bug report" — the same treatment any future secret-adjacent type gets.

### If a local endpoint is ever built anyway

It should not be. If it is:

- Unix socket only, never TCP. A TCP listener on localhost is reachable by every
  process, container, and browser page on the machine.
- The socket file in the data directory at 0600, created after
  `securedirectory` has set the parent to 0700.
- Peer credentials checked — `LOCAL_PEERCRED` on macOS — against `getuid()`, and
  the connection dropped on mismatch. The filesystem mode is the real control;
  this is defence in depth.
- A token generated at first bind, stored 0600, modelled on
  `crates/synapseserve/src/auth.rs`: 32-character minimum, no `Debug`, no
  `Display`, compared in constant time through a digest so the comparison does
  not leak length.
- And it still would not be an improvement over FFI, because every one of those
  controls reduces to "a process running as this user may read the memory," which
  is what the 0600 file already says.

---

## 11. The error model

Today every failure is `anyhow` and the MCP layer collapses it to prose:
`.map_err(|error| error.to_string())` (`crates/synapse/src/mcp/server.rs:62,87`).
An embedder cannot branch on that, so it retries nothing and reports everything as
the same thing.

```rust
pub enum Failure {
    /// `Location::User` and there is no brain on this machine. Distinct from
    /// an empty one — a container hears this instead of getting a fresh store.
    Nobrain { path: PathBuf },
    /// SQLITE_BUSY after the busy timeout. Retryable, and the only one that is.
    Busy,
    /// Someone holds the exclusive lock. A `synapse data restore` is running.
    Locked,
    /// The store is newer than this build understands, or older than it can
    /// migrate. Carries both numbers so the message can say which to upgrade.
    Schema { found: i64, supported: i64 },
    /// `quick_check` or `foreign_key_check` reported damage.
    Damaged { detail: String },
    /// Project scope was asked for and no project root resolved.
    Noproject,
    /// `remember` with content that is empty after trimming.
    Empty,
    /// Owner-only permissions could not be established, or the file is not ours.
    Denied { path: PathBuf },
    /// Anything else, with the chain intact.
    Broken(anyhow::Error),
}
```

### The three the question asks about

**"No memory found" is not a failure.** `recall` returns
`Ok(Answer { memories: vec![] })`. In TS, `answer.memories.length === 0`. This is
already the semantics and it is load-bearing: `searchscoped` returns an empty
`Vec`, and the test named
`a_query_never_matches_on_its_function_words_alone` exists specifically to assert
that returning nothing beats returning a confident wrong match.

**"Database busy" is `Failure::Busy`**, code 3, TS `isbusy(error)`. It is the one
condition an embedder should retry, and the only one — with backoff, and after
`rememberall` has already been tried, since batching is the actual fix (3.2).

**"Scope not approved" turns out to be two different things, and neither is what
it sounds like.**

- *Memory scope* has no approval step. The only refusal is
  `Failure::Noproject`, when `MemoryScope::Project` is asked for and
  `projectroot` returned `None` (`brain/model.rs:39-45`). Note that
  `projectroot` returning `None` is itself `Ok` — "A path that does not resolve
  is not an error, it is the absence of a project"
  (`crates/synapse/src/brain/scope.rs:5-12`) — so recall silently falls back to
  global-only and only a project-scoped *write* refuses.
- *Vault scope* approval is `Resolved.warnings` (`vault/scope.rs:44`), and it is
  **not an error at all**. `vaultstatus` reports it as data:
  `ambient: "blocked"` alongside the warnings
  (`crates/synapse/src/mcp/server.rs:106-112`). The only thing that *errors* on an
  unapproved scope is handing an environment to a child process, which the
  embedding API does not do.

So the answer an embedder gets is: **scope approval is state you read, not an
exception you catch.** Anything designing a `ScopeNotApproved` error class for the
embedding surface has misread the invariant.

### Numeric codes for the C ABI

| Code | Failure |
|---|---|
| 0 | success |
| 1 | `Broken` |
| 2 | `Nobrain` |
| 3 | `Busy` |
| 4 | `Locked` |
| 5 | `Schema` |
| 6 | `Damaged` |
| 7 | `Noproject` |
| 8 | `Empty` |
| 9 | `Denied` |

Append-only, never renumbered — the same discipline `migration::MIGRATIONS`
already follows ("append a new entry and bump `LATEST`, never edit a shipped
one").

```ts
// packages/synapse/errors.ts
export class SynapseError extends Error {
  constructor(readonly code: number, message: string) {
    super(message);
    this.name = "SynapseError";
  }
}

export const isbusy    = (e: unknown) => e instanceof SynapseError && e.code === 3;
export const islocked  = (e: unknown) => e instanceof SynapseError && e.code === 4;
export const isnobrain = (e: unknown) => e instanceof SynapseError && e.code === 2;
export const isschema  = (e: unknown) => e instanceof SynapseError && e.code === 5;
```

(One class, because `Error` subclassing is how JavaScript throws. Everything else
in the SDK is a free function.)

---

## 12. Versioning

Five contracts moving at five speeds. Keeping them separate is the whole job.

| Contract | Version carrier | Breaks when | Cadence |
|---|---|---|---|
| `synapsecore` crate | semver, its own `Cargo.toml` | a Rust signature changes | fast |
| C ABI | `SYNAPSEABI`, one integer, `synapseabi()` | a function is removed or a signature changes | very slow |
| JSON payloads over the ABI | none — additive only | never | continuous |
| MCP tool contract | tool names and descriptions | a tool is renamed/removed, or a description changes meaning | slow, deliberate |
| Database schema | `PRAGMA user_version` / `migration::LATEST` | a migration is appended | rare |

**`synapsecore` versions independently of `crates/synapse`.** Not a preference —
bumping `version` in `crates/synapse/Cargo.toml` on `main` is the release trigger
for the signed and notarized Mac build. An SDK patch must never fire that
pipeline. `synapseserve` already establishes the pattern: "crates version
independently, so a server release must not drag the signed and notarized Mac
build along with it."

**The ABI integer is not derived from any crate version.** Same reasoning
`synapsesync` records for `PROTOCOL`: "deliberately not derived from any crate
version." `synapseabi()` goes 1 → 2 only when a C signature changes, which should
be approximately never, because the payloads carry the change instead.

**Payloads are additive only.** A new field on `Answer` or `Standing` is not a
break — `serde` ignores unknown fields on the way in and TS structural typing
ignores them on the way out. A field is never removed and never retyped. When a
field must go, it is deprecated in the docs and populated with a neutral value
until the next ABI generation.

**The MCP contract moves slowest and most deliberately.** `tests/mcp.rs` asserts
on the exact wording of tool descriptions and server instructions, so changing
them "means updating that test deliberately." That is the right friction, and the
embedding surface must not be a way around it — which is why the C ABI is a
separate surface rather than a second client of the MCP tools.

### The schema pairing rule, which is the one that will actually bite

`migration::run` hard-errors when the file's `user_version` exceeds what the build
knows:

```rust
anyhow::ensure!(
    current <= LATEST,
    "database version {current} is newer than this Synapse release supports"
);
```

For the app that is correct — an old app must not write to a new store. For an
embedder it is a time bomb: a Bun app pinned to an SDK built against schema 7
stops working entirely the day the user updates Synapse to schema 8, with an error
about a database version they never chose.

**Ship the loud failure.** `open()` returns `Failure::Schema { found: 8,
supported: 7 }`, the TS SDK surfaces it as `isschema(error)` with a message naming
which side to upgrade — the same both-directions phrasing `synapseserve` uses:
"this client speaks protocol {presented} and this server speaks {PROTOCOL}"
(`crates/synapseserve/src/http.rs:98`). An embedder checks
`status(brain).schema` and `synapseabi()` at startup and refuses to run rather
than half-working.

**Do not ship the clever version.** The tempting alternative is to let
`synapsecore` *read* a newer schema when every table it touches is unchanged, and
refuse only to write. It is achievable — a per-table fingerprint checked against a
known set — and it is the growth path once there is a second schema in the wild.
It is not the first version, because silently reading a newer store is how an
embedder returns a subtly wrong answer, and a subtly wrong memory is worse than
none.

---

## 13. First customer, end to end

A Bun worker in `tandem` that recalls before it starts and remembers after it
finishes. Both sides, real code.

### The Rust side

`crates/synapseffi/src/lib.rs`, the remember half:

```rust
use std::ffi::{CStr, CString, c_char};
use synapsecore::{Handle, MemoryScope, Remembrance};

#[derive(serde::Deserialize)]
struct Request {
    content: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    scope: Option<MemoryScope>,
    #[serde(default)]
    project: Option<String>,
}

#[derive(serde::Serialize)]
struct Stored {
    id: i64,
}

#[unsafe(no_mangle)]
pub extern "C" fn synapseremember(
    handle: *mut Handle,
    request: *const c_char,
    out: *mut *mut c_char,
) -> i32 {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return code(&Failure::Broken(anyhow::anyhow!("no open brain")));
    };
    let raw = unsafe { CStr::from_ptr(request) };
    let request: Request = match raw.to_str().map_err(Into::into).and_then(readjson) {
        Ok(request) => request,
        Err(error) => return record(Failure::Broken(error)),
    };

    let entry = Remembrance {
        content: request.content,
        source: request.source,
        // Project by default, matching the model-facing guidance.
        scope: request.scope.unwrap_or(MemoryScope::Project),
        project: request.project.map(Into::into),
    };

    match runtime().block_on(synapsecore::remember(handle, &entry)) {
        Ok(id) => {
            let text = serde_json::to_string(&Stored { id }).unwrap_or_default();
            unsafe { *out = CString::new(text).unwrap_or_default().into_raw() };
            0
        }
        Err(failure) => record(failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn synapsefree(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}
```

Note what is not written here: no budget handling, no limit clamping, no scope
resolution. `MemoryScope::project` resolves the root and refuses when there is
none (`crates/synapse/src/brain/model.rs:36-46`); `rememberscoped` trims and
refuses empty content. The FFI layer parses and forwards.

### The TypeScript side

```ts
// tandem/src/memory/index.ts
export { withmemory } from "./session";
export { before, after } from "./work";
```

```ts
// tandem/src/memory/session.ts
import { open, close, isnobrain, type Brain } from "@wess/synapse";

/**
 * One brain for the life of the process. Opening per request would pay a
 * whole-file integrity scan whenever anything else has written, and holding a
 * handle open forever blocks `synapse data restore`, so the shutdown hook is
 * not optional.
 */
let shared: Brain | null = null;

export async function withmemory<T>(work: (brain: Brain) => Promise<T>): Promise<T | null> {
  if (!shared) {
    try {
      shared = await open({ user: true });
    } catch (error) {
      if (isnobrain(error)) {
        // No brain on this machine. Never report a connection that is not there.
        console.warn("synapse unavailable · no brain on this machine");
        return null;
      }
      throw error;
    }
    process.once("beforeExit", () => { if (shared) close(shared); });
  }
  return work(shared);
}
```

```ts
// tandem/src/memory/work.ts
import { recall, remember, isbusy, type Brain, type Memory } from "@wess/synapse";

const project = process.cwd();

/** Recall before work: focused query, smallest practical limit, lean first. */
export async function before(brain: Brain, task: string): Promise<readonly Memory[]> {
  const answer = await recall(brain, {
    query: task,
    project,
    limit: 4,
    budget: "lean",
  });

  // An empty result is an answer, not a failure.
  if (answer.memories.length === 0) return [];

  // `optimization` is what was applied, not what was asked for. Asking for
  // lean under a user configured to lean gives lean; asking for full would
  // still give lean.
  if (answer.optimization !== "lean") {
    console.debug(`synapse · budget resolved to ${answer.optimization}`);
  }
  return answer.memories;
}

/** Remember after work, once the outcome is stable. */
export async function after(brain: Brain, outcome: string, source: string): Promise<void> {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      await remember(brain, { content: outcome, source, scope: "project", project });
      return;
    } catch (error) {
      // The one retryable failure. Everything else is a real problem.
      if (!isbusy(error) || attempt === 2) throw error;
      await Bun.sleep(150 * 2 ** attempt);
    }
  }
}
```

```ts
// tandem/src/worker.ts
import { withmemory, before, after } from "./memory";

await withmemory(async (brain) => {
  const task = "release trigger and homebrew cask update";

  const known = await before(brain, task);
  for (const memory of known) {
    console.log(`· ${memory.source || "memory"}: ${memory.body}`);
  }

  const outcome = await run(task, known);

  if (outcome.stable) {
    await after(
      brain,
      outcome.summary,
      "tandem/release",
    );
  }
});
```

### What the walkthrough demonstrates

- Recall before work, remember after work, with the same discipline the
  model-facing guidance asks of an agent: focused query, smallest practical
  limit, lean budget first.
- The budget is a string the user's configured ceiling constrains. `answer.optimization`
  reports what actually happened.
- Empty is a result. `isbusy` is the only retry. `isnobrain` is a clean
  degradation that says so out loud rather than inventing a count — "never report
  a connection that is not there."
- One handle for the process, closed on exit, so `synapse data restore` still
  works tomorrow.
- Every path funnels to `Brain::recallscoped` and `Brain::rememberscoped`. There
  is no second read path and no way to raise the ceiling.

---

## 14. What this design deliberately does not do

- **No daemon, no port, no socket, no token.** Section 4.
- **No mesh over FFI.** A 240-second park does not belong on an event loop.
  Section 8.
- **No vault in `synapsecore`.** Section 10.
- **No `setoptimization` in the embedding surface.** Section 7, rule 4 — the
  actual hole, and the easiest one to miss.
- **No unbudgeted `search`.** One read path or the invariant is a suggestion.
- **No `globals: false` on recall.** An app that must not see the user's memory
  gets its own database, not a flag. Section 3.4.
- **No `wipe` and no `restore`.** Destruction is a person's decision; restore
  needs a lock a live embedder is holding.
- **No silent tolerance of a newer schema.** Fail loudly, name both versions,
  say which side to upgrade. Section 12.

---

## Appendix: implementation order

1. Extract `crates/synapsecore` — move `database/` (zero cross-module deps) and
   `brain/{model,optimize,scope,settings,store}.rs`; move `CONFIG`; split
   `brain/ingest.rs` into SQL-level free functions here and provider parsers in
   the app crate. No behavior change. `cargo test` at the root must stay green,
   including the two-process `tests/mcp.rs`.
2. Add `Location`, `Care`, `Failure`, and `handle.rs`. `Location::User` refuses a
   missing brain. Expose `database::readonly` for `Location::Readonly`. Map
   `SQLITE_BUSY` to `Failure::Busy` and the exclusive-lock refusal to
   `Failure::Locked`. Add `rememberall`.
3. Add `crates/synapseffi` with `SYNAPSEABI = 1`, the nine entry points, and a
   thread-local last-error. Ship `synapse.h`.
4. Ship `packages/synapse` against ABI 1, with `fallback.ts` spawning
   `synapse mcp` when the dylib is absent.
5. Grow the `exclusivelock` error message to name the process holding the shared
   lock, now that long-lived embedders exist (3.6).

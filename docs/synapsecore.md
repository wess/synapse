# The core library

`synapsecore` is the implementation behind Synapse's desktop app, CLI, terminal
dashboard, and MCP server. The extraction is complete: all non-drawing behavior
lives in `crates/synapsecore`, and `crates/synapse` adds the native window.

## Workspace boundaries

The repository has three independent Cargo workspaces:

| Workspace | Responsibility | Validation |
| --- | --- | --- |
| Repository root | Native desktop and shared sync types | `cargo build --locked` |
| `crates/synapsecore` | Memory, vaults, mesh, plugins, skills, setup, imports, CLI, MCP | `cargo test --locked` |
| `crates/synapseserve` | Optional opaque sync log server | `cargo test --locked` |

A root test does not exercise the other workspaces. Core resolves independently
of the desktop dependency graph and has no dependency on the window or its UI
library. The desktop and core package versions stay equal because they ship as
one product; `synapse version` reads the core package version.

## Public modules

The library's public surface is declared in `crates/synapsecore/src/lib.rs`.

| Modules | Responsibility |
| --- | --- |
| `brain`, `database` | Scoped memory, retrieval, exact reading, migrations, integrity, backups |
| `imports`, `learning`, `usage` | Reviewed imports, feedback and drift resolutions, usage accounting |
| `vault`, `shellsetup` | Credential metadata, sealed values, approved scopes, shell environments |
| `agent`, `files`, `instructions` | Tool descriptors, managed configuration writes, shared guidance |
| `skill`, `sources`, `servers` | Skill and MCP server libraries and their sources |
| `relay` | Mesh messages, roles, teams, process supervision |
| `plugins`, `presence` | Declarative scene packs and live MCP-session rooms |
| `mcp`, `cli` | Protocol and command adapters |
| `sync` | Client operations for the optional encrypted replication protocol |

These are the actual modules, rather than the smaller module set proposed by the
original extraction plan. Core includes command and process orchestration as well
as storage. Embedding the crate does not make it a sandbox.

## Storage and concurrency

`brain::Brain::open` opens a local database through the same migration and
integrity boundary used by the product. `Brain::glance` is the lighter opening path
used for initial display. Keep handles alive while using the store; they retain
the database's advisory lock.

Several local MCP processes can share the same `brain.db`. SQLite WAL coordinates
reads, and competing memory writes are queued through the database write path.
A backup or restore still needs the lifecycle guards. Use the library or the CLI
instead of directly editing tables: indexes, scope metadata, supersession,
redaction, imports, and sync identities must stay consistent.

Memory is plaintext locally. Credential values use a separate encrypted store or
macOS Keychain. Database snapshots do not include those values.

## Minimal build

From the repository root:

```sh
cargo build --release --locked --manifest-path crates/synapsecore/Cargo.toml
./crates/synapsecore/target/release/synapse-cli help
```

For a Rust consumer, use the path to the library workspace:

```toml
[dependencies]
synapsecore = { path = "../synapse/crates/synapsecore", default-features = false }
```

The crate currently declares no optional Cargo features. The no-default-features
gate verifies that this independent build stays valid; it is not a promise that
command, protocol, or platform dependencies are removed.

## Verification

```sh
cd crates/synapsecore
cargo test --locked
cargo check --locked --no-default-features
cargo test --locked --test cli --test mcp
```

See [Embedding](embedding.md) for the transport choice and a scoped Rust example,
[Retrieval checks](retrieval.md) for ranking and exact-read fixtures, and
[Developing Synapse](developing.md) for the complete repository gates.

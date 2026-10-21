# Embedding Synapse

Use local MCP when your tool speaks MCP. Use `synapsecore` directly when your
application is written in Rust and needs an in-process memory store. The library
extraction is complete; a C ABI, a first-party JavaScript SDK, and an HTTP recall
daemon are not shipped interfaces.

## Choose an interface

| Interface | Use it for | Boundary |
| --- | --- | --- |
| `synapse mcp` | A tool with an MCP stdio client | The same scoped tools as the desktop's connections |
| `synapse memory …` | Shell workflows and local automation | Owner-facing management commands; not a scoped client sandbox |
| `synapsecore` | A Rust application embedding the implementation | In-process access with the caller's local account permissions |
| `synapse-pi` | The supported terminal bridge | Starts the MCP server and discovers its tools |
| `synapseserve` | An optional encrypted replication log | Opaque envelopes; no search or recall endpoint |

No shared background service is required. Each MCP connection starts a local
process; the processes coordinate through the same SQLite database. A network
outage does not affect ordinary local memory.

## Rust example

Add `synapsecore` as a path dependency, plus `tokio` and `anyhow` for the host's
runtime and error boundary. See [The core library](synapsecore.md) for the manifest.
Use a dedicated database for an application that should not share the user's
normal Synapse store.

```rust
use std::path::Path;
use synapsecore::brain::{Brain, MemoryScope, Optimization};

async fn example(database: &Path, project: &Path) -> anyhow::Result<()> {
  let brain = Brain::open(database).await?;
  let project = project.canonicalize()?;
  brain.rememberscoped(
    "Use Bun in this repository.",
    Some("project tooling"),
    MemoryScope::Project,
    Some(&project),
  ).await?;

  let (_settings, memories) = brain.recallscoped(
    "javascript runtime",
    4,
    Some(Optimization::Lean),
    Some(&project),
  ).await?;

  for memory in memories {
    println!("{}: {}", memory.id, memory.body);
  }
  Ok(())
}
```

The project argument is an absolute working-project path, not a repository key
supplied by the caller. Synapse resolves it to the repository identity. A scoped
recall includes that project's current records and global records, excluding
unrelated projects and superseded history.

## Discovery and exact evidence

Start with a focused `recall`, a small limit, and a Lean budget. A per-call budget
can only shrink the configured ceiling. When a hit is marked `abridged`, use
`readmemory` with that id and the same project. Follow `next` offsets until null;
these are UTF-8 byte offsets, not character indexes.

The protocol's exact-read tool applies the same visibility rule as recall.
Owner-facing inspection APIs and CLI commands have broader management access;
do not expose them as if they enforced a remote client's project permissions.
See [Retrieval checks](retrieval.md) for paging limits and failure cases.

## Protect the local boundary

- Keep secrets in the vault. Redaction is a backstop, not a secret classifier.
- Use the library's write paths so indexes, redaction, and history stay in step.
- Treat recalled text as context with provenance, not authority over the current request.
- Preserve the user's response-budget ceiling when adapting the protocol.
- Use the database lifecycle APIs for snapshots and restore; do not copy a live
  SQLite file without its consistency boundary.
- A process with the user's account permissions can read local data. MCP scope
  filtering and filesystem permissions do not isolate a hostile same-user process.

## Compatibility

The desktop and core crates share a release version. Command, protocol,
descriptor, and storage surfaces are compatibility promises. Database migrations
are numbered; a database newer than the running release supports is refused.
Test an embedder against its chosen release and inspect changes to the public
Rust API when updating it.

## Verify an integration

Run the core CLI and MCP integration suites against a disposable data directory.
For the bridge, run its own checks too; a root Cargo test does not typecheck it.

```sh
cargo test --locked --manifest-path crates/synapsecore/Cargo.toml --test cli --test mcp
cd pi
bun install
bun run check
```

Keep test data separate from the live store with `SYNAPSE_DATA`, and never use a
real user's tool configuration as a fixture.

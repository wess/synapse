# Encrypted sync

Synapse includes an optional encrypted replication client in `synapsecore::sync`,
wire types in `synapsesync`, and an opaque log server in `synapseserve`. Ordinary
memory, recall, skills, and credential scopes work without any of them configured.

## Current availability

The replication code is a library capability. The app and public CLI currently
provide no sync setup, pairing, or background-sync flow. Running a log server
alone does not connect your desktop to it. Do not edit the live SQLite settings
to simulate a setup flow.

Rust integrations can use the `sync` module's `Config`, `push`, `pull`, and `once`
functions. `once` returns no summary when the store has no complete configuration.
An integration is responsible for configuring and protecting its client state.

## What crosses the network

Clients seal memory operations before they leave the machine. The server stores
opaque envelopes in an append-only sequence; it cannot search memory, inspect
its contents, or resolve conflicts. Clients apply received operations locally.
The wire contract calls for a stable repository key rather than an absolute
checkout path. An integration must verify its project identities before syncing;
the library push operates on the configured store. Use a store whose project
memories have portable repository identities. Path-keyed folders are not portable
project data.

Secret values, local scope approvals, installed tool configuration, worker state,
and learning evidence are outside this memory replication contract. The content
key protects memory from the server. It does not encrypt the local memory store,
which remains plaintext to the local account.

## Server configuration

Build the server from its own workspace:

```sh
cargo build --release --locked --manifest-path crates/synapseserve/Cargo.toml
```

Provide configuration through the service manager's environment. Never put the
token in a command argument, repository, or log.

| Variable | Meaning | Default |
| --- | --- | --- |
| `SYNAPSESERVE_TOKEN` | Required shared access token | None; startup refuses an empty token |
| `SYNAPSESERVE_DATA` | SQLite log file | `synapseserve.db` |
| `SYNAPSESERVE_ADDR` | Listen address | `127.0.0.1:8787` |

Run `crates/synapseserve/target/release/synapseserve` under that configured service.
`GET /health` returns `ok`. `POST /push` and `POST /pull` authenticate access to a
tenant's log. The protocol number is independent of package versions; requests
must match it. Repeated operations are idempotent within their log.

The binary serves plain HTTP on loopback. Terminate TLS before exposing it to a
network: encrypted envelopes protect memory bodies, but the bearer access token
still needs transport protection. The server has request and batch limits;
plan retention and disk capacity for its append-only log.

## Recovery and limitations

A server outage leaves local memory available. Keep independent local backups;
the log is replication infrastructure, not a complete backup of the installation.
A database export excludes credential values and other files needed to rebuild
an installation.

Supersession relationships currently remain local. A memory superseded on one
machine may still be live on another; the wire format does not carry the
replacement relation. Review corrections on every participating device.

## Sync and federation

Sync moves one person's memory between their own devices. Federation would query
another party's explicitly published knowledge. It is a separate proposal, not a
shipped command or a feature of the blind log server. See the
[Federation proposal](federation.md) for that design's status and trust model.

## Verification

```sh
cargo test --locked --manifest-path crates/synapseserve/Cargo.toml
cargo test --locked --manifest-path crates/synapsecore/Cargo.toml --test sync
```

The protocol and encryption fixtures live in `crates/synapsesync`; root Cargo
tests cover that workspace member.

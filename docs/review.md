# Security, stability, and resource review

Reviewed on 2026-10-26, with dependency policy refreshed on 2026-11-12. Scope: Project Knowledge, its MCP/session integration, and the three dependency lockfiles. This is a source review and local validation, not a penetration test or a benchmark of the entire desktop app.

## Fixes

- Redact the complete overview before taking an excerpt. Truncating first could expose the beginning of a credential or cut a private marker in half.
- Return generic metadata errors instead of parser diagnostics containing document text. Bound invalid-file warnings and serialized index size.
- On Unix, open guides, sources, and locks through pinned directory handles with no-follow and regular-file checks. Atomic replacement and directory enumeration use those same handles. A directory replaced by a symlink cannot redirect the operation. Pipes cannot block file reads. On Windows, retain ancestor directory handles without delete sharing, reject reparse points, and check the opened file's metadata.
- Restrict knowledge reads and writes to the MCP process's project root. A connected client cannot use Synapse's filesystem authority to reach another checkout.
- Use a nonblocking exclusive writer lock. Contention returns a retry response rather than tying up a session indefinitely. Existing revision checks remain in place.
- Stream source hashes through a 64 KiB buffer, deduplicate hashes within each request, and cap total source reads at 16 MiB. There is no cache across calls, so same-size edits are rechecked. Guides with unverified sources report `unchecked` instead of `current`.
- Limit index inspection to 128 directory entries and 32 guides; cap serialized catalog output at 32 KiB. Keep full guide reads paginated.
- Execute knowledge filesystem work on the blocking pool, with at most two jobs per process and no unbounded waiting queue. No watcher or periodic scan was added. Disabled knowledge does not inspect guide files.
- Presence slot selection uses sets and liveness checks are batched once per snapshot rather than performed while rendering.

## Dependency updates

Updated compatible patch versions where present in each lockfile:

| Package | Patched version |
| --- | --- |
| h2 | 0.4.16 |
| quinn-proto | 0.11.18 |
| rustls | 0.23.46 |
| rustls-webpki | 0.103.16 |
| webpki-roots | 1.0.10 |
| tokio | 1.53.3 |
| chacha20 | 0.10.2, replacing a yanked version |

The configured RustSec audit reports no unacknowledged advisories in all three lockfiles. The core and sync server have no advisory warnings after these updates.

The desktop lockfile has 14 upstream maintenance warnings. They enter through the current UI release's executor, archive, HTTP, platform, graphics, text, and SVG dependency graphs. None describes a known vulnerability and none offers a patched version. Replacing them locally would mean carrying private forks of those layers, so the audit policy acknowledges their exact advisory IDs and rejects every new warning. The exceptions are due for review on every UI release and no later than 2027-02-12.

Two compiler-compatibility warnings remain in `block` and `proc-macro-error2`. The first belongs to the macOS platform bindings; the second enters through the UI crate's stack-growth support. Both require upstream dependency changes. Current upstream source has already made stack-growth support optional, but that work is not in a compatible release yet.

The bundled SQLite advisory remains mitigated at the trust boundary: restore refuses a database with adjacent write-ahead-log state before SQLite opens it. SQLx 0.9 still caps `libsqlite3-sys` below the patched 0.39 release, so there is no compatible upgrade. This exception stays explicit until SQLx accepts a patched bundled SQLite.

CI now audits all three independent lockfiles and fails on any advisory or yanked package outside the reviewed list. Weekly dependency updates cover each Cargo workspace so compatible upstream fixes arrive without waiting for a manual sweep.

## Synthetic resource measurements

Optimized local benchmark, 40 index requests, disposable fixture with 32 guides sharing 64 source files (4 MiB total):

| Measurement | Result |
| --- | --- |
| Empty index median / p95 | 0.32 / 0.48 ms |
| Populated index median / p95 | 15.58 / 17.11 ms |
| Source bytes read per populated request | 4 MiB, down from 128 MiB without deduplication |
| Serialized populated index | 7,771 bytes |
| Benchmark process maximum RSS | 7,241,728 bytes, about 6.9 MiB |

RSS includes fixture generation and the benchmark process; it is not the desktop app's memory usage. The benchmark used optimized code with release stripping disabled to avoid a local macOS SQLx procedural-macro loader error. These numbers are filesystem- and machine-dependent; network or unhealthy filesystems can take longer. The read budget bounds bytes and concurrency, not kernel I/O latency. Process-local limits also do not impose a machine-wide cap across independently launched agents.

## Validation

- Full headless core, desktop/workspace, and sync-server test suites.
- MCP end-to-end create/read/disable behavior and session handoffs.
- Regression tests for redaction boundaries, private content, parser errors, invalid-file floods, per-request byte limits, source deduplication, lock contention, replaced directories, special files, and symlinked locks.
- A current-thread Tokio test verifies that occupied knowledge workers do not block the async executor and that a third job is refused while two are active.
- Strict Clippy and formatting checks for the affected workspaces.
- Windows core cross-compilation with a local Zig C compiler. Windows native runtime and filesystem behavior were not exercised on a Windows machine in this review.

## Devcontainer release audit

Reviewed on 2026-11-25. This review covers SSH enrollment, the host-side MCP
session, release regressions, and all three workspace lockfiles.

### Resource use

- Enrollment verifies the full host store. A connection checks database
  relationships and migrations at startup without scanning every SQLite page.
  No Synapse background process is needed between connections.
- Eight file-lock slots bound simultaneous SSH MCP sessions per host account.
  A session may run two memory calls at once and refuses additional calls for
  the client to retry. A handshake must finish within ten seconds.
- MCP discards incoming frames over 2 MiB. Recall limits queries to 4 KiB and
  uses Balanced or a smaller budget. Exact reads remain paginated. Writes
  allow up to 256 KiB of content, a 2 KiB source label, and 64 correction IDs.

In a synthetic loopback OpenSSH run on arm64 macOS, the debug CLI opened a
session in 71 ms. With 128 project memories of roughly 1 KiB each, 100 sequential
Lean recalls took 5.49 ms median and 5.83 ms p95. The host MCP process used
23.4 MiB resident memory at one post-run sample. These figures exclude the SSH
daemon and client. They describe this fixture rather than a whole-app
benchmark or a bound on performance for larger stores.

### Security

- Each grant enrolls an owner-only Ed25519 identity and pins the host public
  key. The authorized entry forces the restricted MCP command and disables
  shell facilities such as forwarding and TTY allocation. Only the generated
  client bundle is mounted in the container.
- The server uses the host-approved project identity for search, exact reads,
  writes, and corrections. Global memory, other projects, and vault, mesh,
  and learning tools are outside that grant. Responses omit the absolute
  host path used as the identity of a project with no network Git origin.
- All three lockfiles use `url` 2.5.9 for
  [RUSTSEC-2026-0592](https://rustsec.org/advisories/RUSTSEC-2026-0592.html).
  The desktop audit records the maintenance-only
  [Chrono notice](https://rustsec.org/advisories/RUSTSEC-2026-0543.html)
  while the current MCP and schema dependencies still require it.
  Fresh RustSec checks found no unacknowledged vulnerabilities.

### Stability and validation

Enrollment preserves unrelated SSH key entries and backs up changes. A rerun
reuses the project's private key and registration. Revoking the grant blocks
later calls, and a recreated grant gets a new identity. Close open clients
to release their session slots and database lifecycle locks before restoring
the store.

The MCP integration test exercises forged project parameters, isolation for
both ranked and empty-query recall, exact reads and correction IDs, resource
limits, and revocation across grant recreation. A disposable OpenSSH daemon
also verified host-key pinning, forced-command execution, path omission, and
revocation for active and new sessions. A full VS Code container rebuild was
outside this audit. See [devcontainer setup](devcontainer.md) for the
installation and troubleshooting steps.

# Security, stability, and resource review

Reviewed on 2026-10-26. Scope: Project Knowledge, its MCP/session integration, and the three dependency lockfiles. This is a source review and local validation, not a penetration test or a benchmark of the entire desktop app.

## Fixes

- Redact the complete overview before taking an excerpt. Truncating first could expose the beginning of a credential or cut a private marker in half.
- Return generic metadata errors instead of parser diagnostics containing document text. Bound invalid-file warnings and serialized index size.
- On Unix, open guides, sources, and locks through pinned directory handles with no-follow and regular-file checks. Atomic replacement and directory enumeration use those same handles. A directory replaced by a symlink cannot redirect the operation. Pipes cannot block file reads. On Windows, retain ancestor directory handles without delete sharing, reject reparse points, and check the opened file's metadata.
- Use a nonblocking exclusive writer lock. Contention returns a retry response rather than tying up a session indefinitely. Existing revision checks remain in place.
- Stream source hashes through a 64 KiB buffer, deduplicate hashes within each request, and cap total source reads at 16 MiB. There is no cache across calls, so same-size edits are rechecked. Guides with unverified sources report `unchecked` instead of `current`.
- Limit index inspection to 128 directory entries and 32 guides; cap serialized catalog output at 32 KiB. Keep full guide reads paginated.
- Execute knowledge filesystem work on the blocking pool, with at most two jobs per process and no unbounded waiting queue. No watcher or periodic scan was added. Disabled knowledge does not inspect guide files.

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

The configured RustSec audit reports zero vulnerabilities in all three lockfiles. The core and sync server have no advisory warnings after these updates.

The desktop lockfile still has 13 upstream maintenance-warning entries. Two existing suppressions remain for quick-xml 0.30: RUSTSEC-2026-0194 and RUSTSEC-2026-0195. An audit without those suppressions confirms both advisories. They are retained for GPUI's optional xcb build dependency; `cargo tree --target all -i quick-xml@0.30.0 --locked --offline` reports no dependency in the currently enabled graph. No additional suppression was introduced. Updating that optional graph requires an upstream-compatible dependency change.

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

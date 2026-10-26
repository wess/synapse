# Project Knowledge

Project Knowledge gives connected coding agents a shared, repository-local guide to an application. The agent examines the code and writes Markdown; Synapse makes the guides available across sessions and tools. There is no model training, background agent, or additional model provider to configure.

## Enable

In the desktop app, open **Settings → Project Knowledge → On**, or run:

```sh
synapse settings knowledge on
```

The feature is off by default. Restart connected tools after enabling so they load the two knowledge tools. pi receives the same tools through its existing Synapse extension.

From a connected agent working in your repository, ask:

> Have Synapse learn this project. Inspect its stack, architecture, conventions, database, and tests. Create an overview and focused guides, cite the source files, and tell me what remains uncertain.

The agent uses its existing source-reading tools and model. Synapse supplies the authoring guidance and stores the resulting guides. It does not independently scan your repository or launch another agent.

## Files and session behavior

Guides live in `.synapse/knowledge/`, for example:

```text
.synapse/knowledge/overview.md
.synapse/knowledge/architecture.md
.synapse/knowledge/database.md
.synapse/knowledge/testing.md
```

Each file has TOML frontmatter containing a short description and SHA-256 hashes of the inspected source files, followed by its Markdown body. You can edit the Markdown in your usual editor and include the guides in normal code review. Synapse also creates `write.lock` in that directory to coordinate concurrent saves; it contains no project content and does not need to be versioned.

Session hooks deliver a compact guide index and an excerpt of the overview, including after compaction. Agents using MCP without a session hook are instructed to request the index at startup; ordinary memory recall also includes the index while the feature is enabled. Complete guide bodies are read on demand through `knowledge`, in pages of at most 6,000 UTF-8 bytes. The index includes at most 32 guides and an overview excerpt of at most 1,800 bytes.

Guides describe the checkout being used. They are not stored as global memories or shared by repository identity across different worktrees. Existing project decisions still belong in memory, and repeatable procedures still belong in skills.

## Refresh

Ask your connected agent:

> Refresh Synapse's database guide against the current migrations. Preserve my edits and summarize the changes.

Synapse compares tracked source hashes whenever the index or a guide is requested. A guide marked `needsreview` has changed, missing, or unreadable source files. A guide marked `unchecked` reached the per-request source budget; its `unchecked` count tells you how many sources remain unverified. Treat both states as requiring source inspection. `current` means those tracked files match; it does not prove the explanation is accurate or account for newly added files that were never tracked. Source code remains authoritative.

Refresh is explicit. Synapse does not rewrite guides when a source changes. The agent must read the existing guide and supply its revision when saving. If another agent or a person edited it in the meantime, Synapse refuses the save until the agent rereads and reconciles the changes. The revision protects against an intervening edit; review the agent's diff to confirm it preserved the intent of your text.

Names contain lowercase letters and digits. Guides require 1–64 inspected repository-relative source files, with a 2 MiB limit per source and a 16 MiB aggregate source-read budget per request. Guide bodies are limited to 128 KiB. Credential files and Synapse metadata cannot be tracked sources, and recognized credentials in generated text are rejected. These checks do not replace reviewing what an agent writes.

## Resource use and filesystem access

Knowledge work runs only when requested or handed over at session startup. There is no watcher, polling loop, background model, or persistent source cache. Disabling the setting skips guide access entirely.

Each request streams source hashes through a 64 KiB buffer and hashes shared files once, even when multiple guides reference them. The index examines at most 128 directory entries and 32 guide files, caps source reads at 16 MiB, and limits serialized index output to 32 KiB. Oversized or unverified input is reported instead of silently being treated as current. At most two knowledge jobs run at once per process; additional requests receive a retry message. Filesystem work runs outside the async executor. A busy writer also returns a retry message rather than waiting indefinitely.

Knowledge access is restricted to the project root supplied to the MCP process through `SYNAPSE_PROJECT_DIR`, falling back to its starting directory. A tool may pass that root or a folder inside it, but it cannot use Synapse to read or write guides in another checkout. Guide paths and source components cannot be symlinks. On Unix, access and atomic writes use pinned directory handles, so replacing a directory with a symlink during a request cannot redirect that operation outside the opened repository. On Windows, directory handles are held without delete sharing and reparse-point files are rejected. Source files must be regular files; pipes and other special files are rejected. Metadata errors do not echo document contents, and overview excerpts are redacted before truncation.

To repeat the synthetic performance check in a disposable repository:

```sh
CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none CARGO_PROFILE_RELEASE_STRIP=none \
  cargo run --manifest-path crates/synapsecore/Cargo.toml --release --example knowledge
```

It measures an empty index and 32 guides referencing the same 64 source files (4 MiB total), reporting median and 95th-percentile latency, bytes read, and response size. These are local synthetic measurements, not a guarantee for slow or remote filesystems.

## Disable

Choose **Settings → Project Knowledge → Off**, or run:

```sh
synapse settings knowledge off
```

Synapse immediately refuses further knowledge-tool calls and stops including guides in new session handoffs and memory recall. Restart connected tools to remove the tool definitions and guidance from their next session. Context already delivered to an active agent cannot be withdrawn. Existing Markdown files remain in the repository and are still available through ordinary file access.

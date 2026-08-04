# Synapse product brief

## Purpose

Synapse is a quiet local memory service for developer tools. It gives every connected coding tool the same durable, inspectable memory without requiring a general-purpose notes application, and lets those tools coordinate with each other through the same local store.

## Primary users

Developers who move between multiple coding tools and want decisions, preferences, corrections, and project context to carry across sessions.

## Core loop

1. Connect a supported tool once.
2. The tool says so at startup, recalls relevant memory before work, and stores durable context after work.
3. Optionally, connected tools join one mesh: they message each other, hand work back and forth, and park for free between tasks.
4. The user can inspect health, usage, connections, and the mesh from a small native dashboard.
5. The user grants credential names to one command or an opted-in shell through explicit global or YAML-backed scopes.

## First release

- A local SQLite memory store.
- Search, inspection, editing, deletion, and guarded wipe controls for stored memory.
- Enforced global and project memory scopes shared by every connected tool.
- Previewed, idempotent, reversible imports from Claude, Codex, and selected Markdown.
- An MCP stdio server exposing `remember`, `recall`, and value-free vault status.
- An opt-in agent mesh over the same local database: register, direct messages, channels, broadcasts, free parking on `wait`, reported work state, and supervised background workers. Off by default, because its tools cost context in every session that loads them.
- Reusable agent roles and team rosters as layered TOML, resolved project, then user, then built-in.
- One Agent Skills library installed into every connected tool, so a skill is written once instead of copied by hand into each one and left to drift. Skills Synapse did not install are reported, never overwritten.
- One-click user-level setup for Codex, Claude Code, and pi, including a session notice and status line that state the connection before the model has written anything. pi has no MCP client, so its connection is the `synapse-pi` package, which carries the tools, the notice, and the guidance together.
- One editable `SOUL.md` for shared guidance, with managed pointers in each tool's global instruction file.
- Safe pointer sync plus explicit, backed-up consolidation of existing global guidance.
- Keychain-backed vaults managed from the dashboard.
- Global, project, and folder resolution through approved `.synapse.yaml` files.
- `synapse run -- <command>` for one-child scoped environment injection.
- One-click Settings management for optional zsh, bash, and fish hooks, with automatic loading in explicitly approved directories and `allow` and `deny` controls.
- A user-installable CLI with safe secret prompting and the same vault/scope controls as the GUI.
- Non-destructive recall optimization with Full, Balanced, and Lean response budgets, plus per-call reductions that cannot exceed the user's configured ceiling.
- Light and dark themes that follow the operating system by default.
- Clear detection, connected, missing, success, and error states.
- Numbered migrations, integrity checks, owner-only data permissions, backups, export, and restore.
- A signed Apple-silicon macOS 13+ beta archive with a user-installable CLI launcher.

## Operating context

Native desktop application, macOS first. It should feel at home beside a terminal: compact, calm, fast, and useful at a glance. The integration registry and storage code should remain portable.

## Design direction

Use a warm, low-contrast canvas with a crisp white working surface, ink typography, and a restrained violet accent. Connections are rows in one coherent console, not a collection of decorative cards. Status must be readable without relying on color alone.

## Constraints

- Rust, Tokio, GPUI, Guise, and SQLx with SQLite.
- Local-first. No account or network service is required.
- Preserve user-owned configuration and instruction content.
- Never import conversation logs, settings, authentication files, or credential-shaped memory without explicit review.
- Never write secret values to SQLite, YAML, MCP responses, or the application log.
- Never activate an ambient environment from global mappings alone or from an incomplete scope; unload on invalidation and restore pre-existing shell values.
- Keep source files small, lowercase, and grouped by responsibility.
- Prefer data and functions over class-like abstractions.

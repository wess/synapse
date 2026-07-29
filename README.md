# Synapse

Your tools forget. Synapse remembers.

Synapse keeps project decisions and credentials on your Mac, ready for the tools and terminal sessions that need them. There is no account to create and no cloud memory to manage.

[Download the macOS beta](https://github.com/wess/synapse/releases/download/v0.1.0-beta.8/synapse.zip) · [Read the guide](https://wess.io/synapse/docs/)

Apple silicon · macOS 13 or later · Developer ID signed and notarized

## What Synapse does

- **Keeps the thread.** Save decisions, corrections, conventions, and preferences once. Pick them up in a later session or another connected tool.
- **Brings history with you.** Preview and import existing Claude and Codex memory into project-scoped Synapse records without changing the originals.
- **Shares one playbook.** Keep global working guidance in one editable `SOUL.md`, with both tools pointed at it.
- **Scopes credentials.** Keep secret values in macOS Keychain and choose which approved folders may receive which environment variables.
- **Leaves you in control.** Search, edit, export, restore, or delete what Synapse stores. Nothing is hidden behind an account or remote service.

## A simple workflow

1. Install Synapse and connect the tools you use.
2. Import useful existing memory, then let Synapse remember confirmed context that will matter later.
3. Return to the project with its decisions already available.
4. Run commands with only the credentials that project is allowed to use.

Use a scoped environment for one command:

```sh
synapse run -- your-command
```

Or enable shell integration in Settings to load approved project environments when you enter their folders.

## Learn more

- [Install and connect](https://wess.io/synapse/docs/install/)
- [Memory and recall](https://wess.io/synapse/docs/memory/)
- [Credentials and project scopes](https://wess.io/synapse/docs/vault/)
- [Complete tutorials](https://wess.io/synapse/tutorials/)

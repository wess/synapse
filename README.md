# Synapse

Your tools forget. Synapse remembers.

Synapse keeps project decisions and credentials on your Mac, ready for the tools and terminal sessions that need them. There is no account to create and no cloud memory to manage.

[Download for macOS](https://github.com/wess/synapse/releases/latest/download/synapse.zip) · [Read the guide](https://wess.io/synapse/docs/)

Apple silicon · macOS 13 or later · Developer ID signed and notarized

## What Synapse does

- **Keeps the thread.** Save decisions, corrections, conventions, and preferences once. Pick them up in a later session or another connected tool.
- **Says so at startup.** Claude Code shows `Synapse connected · 128 memories` beside its welcome message, so you can see the link before the first reply.
- **Asks before it forgets.** When a long session is about to be compacted, Synapse asks it to write down anything it worked out that is not stored yet — the one moment where not having written something down costs you immediately.
- **Corrects without arguing.** When a convention changes, the new memory supersedes the old one instead of contradicting it. Recall returns the current version; the old text stays readable and comes back if you were wrong.
- **Brings history with you.** Preview and import existing Claude and Codex memory into project-scoped Synapse records without changing the originals.
- **Shares one playbook.** Keep global working guidance in one editable `SOUL.md`, with every connected tool pointed at it.
- **Writes a skill once.** Keep your Agent Skills in one library and install them into Claude Code, Codex, pi, and Ainz together, instead of copying folders by hand and watching the copies drift apart. A skill about one repository belongs to that repository.
- **Learns a procedure.** Let a session write down something it worked out as a skill, and correct one that turned out wrong. What an agent writes waits for you to approve it and reaches no tool until you do. Off by default.
- **Lets agents work together.** Turn on the mesh and your connected tools can message each other, split up a job, and wait for free between tasks. Off by default.
- **Gives you a seat at the table.** The Console puts you on the mesh under your own name, so an agent that hits a decision it should not make alone has somebody to ask — and every worker stays directly addressable rather than reachable only through a lead.
- **Scopes credentials.** Keep secret values in an encrypted store this machine owns — or in macOS Keychain, whichever you choose — and pick which approved folders may receive which environment variables. A value never reaches a project file, a log, or a response, and comes back out only onto your clipboard.
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

## One skill library

Claude Code, Codex, [pi](https://pi.dev), and [Ainz](https://github.com/wess/ainz) all read the [Agent Skills](https://agentskills.io) format, each from its own folder. Synapse keeps one copy and installs it into each:

```sh
synapse skill list                # what is in your library
synapse skill adopt humanize      # bring in a skill a tool already has
synapse skill create my-workflow  # start a new one
synapse skill install             # copy them all into every connected tool
synapse skill status              # where each one is, and what has drifted
```

Editing a skill in the library marks the installed copies as out of date; `install` brings them back in step. A skill Synapse did not put there is left alone.

A procedure that is really about one repository belongs to it rather than to every session on the machine:

```sh
synapse skill create release --project   # this repository's own
synapse skill install release --project  # into its .claude/skills, not your home
```

## Letting agents improve themselves

Turn it on and a session can write down a procedure it worked out, and correct one that turned out wrong:

```sh
synapse settings learn on
synapse skill proposed            # what agents wrote and nobody has looked at
synapse skill approve cut-a-release
synapse skill history cut-a-release   # what it used to say
synapse skill revert cut-a-release    # and back again
```

The gate is on installing rather than on writing. A taught skill sits in the library and in no tool until you approve it, so writing one costs you a line in a list rather than context in every session on the machine. Corrections are the deliberate exception: they reach the copies Synapse installed, because you already agreed to that skill being loaded and a correction that never arrives leaves every session running the version that was wrong. Nothing it replaces is lost.

## Working as a team

Turn the mesh on in **Settings → Agent mesh**, or from the terminal:

```sh
synapse settings mesh on
synapse relay team open web      # a lead in this terminal, its team in the background
synapse relay agents             # who is on the mesh and what they are doing
synapse relay feed --follow      # watch them talk
```

Each agent launches with a role — a durable brief describing what it owns. The built-in roles cover the usual shape of a team, and `synapse relay role create <name>` writes your own into the project so it travels with the checkout.

Most jobs do not want four agents, and picking a roster before you understand the job is its own small chore. The `overseer` team is one agent that grows its own:

```sh
synapse mux --team overseer

@overseer get the release notes written and the changelog updated
```

You are on the roster yourself either way, so nothing is interposed: any worker it starts is directly addressable without going through it. The app has the same seat on its **Console** screen, with a transcript, the roster, and a box to type in.

## Using pi

[pi](https://pi.dev) has no MCP client, so it reaches Synapse through a package instead:

```sh
synapse connect pi               # or, from pi's side: pi install npm:synapse-pi
synapse launch pi                # one session with memory, the vault, and the mesh
```

Everything a connection means elsewhere arrives with that package: the tools, this project's memory before the first turn, a status line, `/synapse`, `/recall`, `/remember`, and `/mesh`. The source is in [`pi/`](pi/).

## Learn more

- [Install and connect](https://wess.io/synapse/docs/install/)
- [Memory and recall](https://wess.io/synapse/docs/memory/)
- [Credentials and project scopes](https://wess.io/synapse/docs/vault/)
- [Complete tutorials](https://wess.io/synapse/tutorials/)

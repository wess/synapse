# Synapse

**Shared memory and skills for Claude Code, Codex, pi, and Ainz.**

Stop repeating the same project conventions to each coding agent. With Synapse, you can save a correction in one tool and recall it in another, in the same project. Keep your memory on your machine and manage reusable skills in one library.

[Download for macOS](https://github.com/wess/synapse/releases/latest/download/synapse.zip) · [Linux setup](https://wess.io/synapse/docs/install/#linux) · [Try a handoff](https://wess.io/synapse/tutorials/continuity/) · [Documentation](https://wess.io/synapse/docs/)

macOS desktop (Apple silicon, macOS 13+, signed and notarized) · Linux terminal dashboard, CLI, and MCP server · MIT license · No account required

## Supported harnesses

A harness is the coding agent you run. Synapse ships these connectors:

| Harness | Command | Connection | Extra integration |
| --- | --- | --- | --- |
| Claude Code | `claude` | MCP | Session hooks and status line |
| Codex | `codex` | MCP | Respects `CODEX_HOME` |
| pi | `pi` | `synapse-pi` extension | Session integration; no native MCP client needed |
| Ainz | `ainz` | MCP | Registers Synapse as a required server |

All four have descriptors for shared guidance, skill locations, and launch arguments. Run `synapse tool list` to see built-in and custom harnesses, then `synapse connect <name>` to connect one installed on your machine.

### Add your own harness

You can add a harness without rebuilding Synapse or waiting for a release. From your repository root:

```sh
synapse tool create mytool
synapse tool show mytool
synapse connect mytool
```

The first command opens a commented TOML template in your editor and validates it before saving **`.synapse/tools/mytool.toml`**. Fill in your harness's executable, configuration and instruction paths, MCP registration commands, and launch flags. Restart the harness after connecting, then ask it to save and recall a project convention.

Use `synapse tool create mytool --user` for a personal connector available across projects. Project descriptors override user descriptors, which override built-ins. Run project commands from the repository root. `.synapse.yaml` remains the project credential-scope configuration; harness definitions live in `.synapse/tools/`.

[Custom harness guide and example](https://wess.io/synapse/docs/harnesses/). Support for hooks or a non-MCP protocol may require an adapter in addition to a descriptor.

### Contribute a harness

**PRs for additional harnesses are welcome.** Start with a working custom descriptor, then add it to [`crates/synapsecore/assets/tools/`](crates/synapsecore/assets/tools/) and register it in `BUILTINS` in [`agent/tool.rs`](crates/synapsecore/src/agent/tool.rs). Include the harness version and platforms you tested, coverage for connection detection and cleanup, and an update to this table. See the [contribution checklist](https://wess.io/synapse/docs/harnesses/#contribute).

## Save a correction. Recall it in another agent.

An example with two connected tools in the same repository:

```text
You → Codex
  Use Bun for JavaScript tasks in this repo.
  Remember that convention for this project.

Synapse
  Stores the confirmed convention in local project memory.

You → Claude Code, in a new session
  Recall this project's tooling convention.
  How do I install dependencies?

Claude Code, using the recalled convention
  bun install
```

You choose which decisions to keep. Connected agents can recall those decisions across sessions; project-scoped recall includes global guidance and that project's memory. Synapse shares saved context, not the full conversation. An agent still needs to consult memory and follow it.

[Walk through this example](https://wess.io/synapse/tutorials/continuity/), including how to inspect the saved record, correct it, and check its project scope.

## Try it with your tools

On macOS:

1. Download the app, extract it, and move **synapse.app** into **Applications**.
2. Open Synapse and choose **Connect** for the tools you use: Claude Code, Codex, pi, or Ainz.
3. Restart those tools so they load the connection and shared guidance.
4. Ask one tool to remember a confirmed project convention. Open another in the same folder and ask it to recall that convention.
5. Inspect the record in Synapse's **Memories** screen.

On Linux, build the terminal version from the repository root with a current stable Rust toolchain and a C compiler/linker:

```sh
cargo build --release --locked --manifest-path crates/synapsecore/Cargo.toml
./crates/synapsecore/target/release/synapse-cli install
export PATH="$HOME/.local/bin:$PATH"
synapse connect
synapse
```

`synapse connect` connects installed tools. Restart them afterward. Running `synapse` in a terminal opens the dashboard; the same binary provides the CLI and MCP server. The Linux build has no desktop UI dependency and uses the encrypted vault instead of macOS Keychain. Current release downloads contain the macOS app; Linux users build from source.

See the [installation guide](https://wess.io/synapse/docs/install/) for CLI setup and connection troubleshooting.

## What you can share

| Capability | How you use it |
| --- | --- |
| Project memory | Save decisions, corrections, and conventions. Recall them in a later session or another connected agent. |
| Existing memory | Preview and import Claude and Codex memory without changing the originals. Undo an import batch when needed. |
| Shared guidance | Edit one `SOUL.md` and point your connected tools at it. |
| Agent Skills | Maintain one library, install skills into connected tools, and check which copies have drifted. |
| Agent coordination | Enable the optional mesh to let agents message each other and work together. Join them through the Console. |
| Scoped credentials | Approve which project folders may receive which environment variables, using the local encrypted vault or macOS Keychain. |
| The Map | See every memory as a graph in three dimensions — projects as clusters, a correction as a line to what it replaced, and a memory nothing else touches looking like one. |

Search, edit, export, restore, or delete stored memory from the app. Supersede a decision when it changes: future recall returns the current version, and you can still inspect or restore the old one.

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

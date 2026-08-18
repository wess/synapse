# synapse-pi

Synapse for [pi](https://pi.dev): durable local memory, scoped credential
metadata, and the agent mesh, as native pi tools.

[Synapse](https://wess.io/synapse/) keeps developer memory in local SQLite, lets
connected coding agents coordinate over that same database, and brokers
Keychain-backed credentials into scoped environments. Local-first: no account, no
network service, nothing leaves the machine.

```sh
pi install npm:synapse-pi
```

It needs the `synapse` command on the machine. If it is missing, the session says
so once and carries on with no Synapse tools — a connection that is not there is
never reported as one that is.

[Download it](https://github.com/wess/synapse/releases), then put the command on
PATH:

```sh
synapse install
```

## What a session gets

**Memory that is already there.** Before the first turn, this project's memory is
recalled and handed to the model, and you get one line saying what was found.
Asking a model to recall on its own is guidance it may or may not follow.

**Every tool Synapse offers.** `remember`, `recall`, and `vaultstatus` always. The
sixteen mesh tools — `register`, `send`, `post`, `broadcast`, `wait`, `spawn` and
the rest — appear too, but only while the mesh is switched on:

```sh
synapse settings mesh on
```

Nothing in this package decides that list. It asks the server what exists and
registers whatever comes back, so turning the mesh on adds the tools on the next
start with no version of this package to update.

**Slash commands**, for the questions you want answered yourself rather than
through a turn:

| Command | What it does |
| --- | --- |
| `/synapse` | What Synapse holds for this project: memory, vault, mesh, tools |
| `/recall <query>` | Search durable memory |
| `/remember <fact>` | Store one durable fact |
| `/mesh` | Who is on the mesh right now |

**A nudge before compaction.** When pi is about to compact the session, the
extension asks it to carry out an explicit list of anything durable it settled
that Synapse does not already hold, and to `remember` each one. Compaction is
the one moment where not having written something down costs immediately. The
compaction itself is never cancelled or rewritten.

**A status line**, the same one Synapse shows in every other connected tool, and
the guidance that explains the tools — skipped when `synapse connect pi` has
already put it in `~/.pi/agent/APPEND_SYSTEM.md`.

## Credentials

Secret values live in the macOS Keychain and never reach this extension. What it
can show you is metadata — which variable names are available for a folder,
whether the `.synapse.yaml` scope there is approved — which is what `vaultstatus`
answers.

To hand a session the values themselves, start it through Synapse:

```sh
synapse launch pi              # memory, vault, and this package, for one session
```

That resolves the folder's scope and gives the child process the environment. It
refuses when a scope is unapproved or has changed, because a tool that can run a
shell is never handed a half-resolved environment.

## Being part of a team

With the mesh on, a pi session is a full participant: it can register, be
addressed by name, join channels, and spawn headless workers of its own.

```sh
synapse launch pi --as frontend --role frontend   # join the mesh as `frontend`
synapse relay launch backend --tool pi            # start another agent on pi
synapse mux --team web                            # drive the team from a terminal
```

A row on the roster marked as a person is somebody at a keyboard: agents are told
to ask them questions and never delegate to them.

## Where the binary comes from

`SYNAPSE_COMMAND`, if it is set to an absolute path — this is how a session that
Synapse itself started is told which binary its memory belongs to. Otherwise
`PATH`, then `~/.local/bin`, `~/.cargo/bin`, `~/.asdf/shims`, `/opt/homebrew/bin`
and `/usr/local/bin`.

## Working on it

The package is one extension in the Synapse repository, under `pi/`. Point pi at
a checkout to run it without publishing:

```sh
pi install /path/to/synapse/pi
```

MIT licensed, like the rest of Synapse.

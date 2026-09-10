# What launching costs

Numbers from an Apple silicon Mac, release build, a store of 220 memories, with
Claude Code, Codex and Ainz installed and connected. They are here so that a
change can be compared against something rather than described.

## The window

Time from `exec` to the first painted frame: **about 200ms**, of which Synapse's
own reads are **about 6ms**.

| Phase | Cost | Whose |
| --- | --- | --- |
| Process start, dyld, gpui framework init | ~85ms | the platform |
| `Dashboard::new` — one `Brain::glance`, every setting, one memory list | ~6ms | ours |
| Window creation, Metal pipeline setup | ~75ms | the platform |
| First frame | ~35ms | the platform |
| **After the frame:** tool detection, import scan, integrity check | ~60ms | ours, deferred |

The split is the design. `Dashboard::new` reads only what the first frame needs.
Everything whose cost belongs to another program runs in `Dashboard::opened`,
scheduled by `window.on_next_frame`:

- **Tool detection.** Every installed coding agent is asked for its `--version`,
  and several start a Node or Bun process to answer. This is unbounded from
  Synapse's side — it is somebody else's startup time — and it used to be the
  first thing that happened.
- **The import scan**, which walks Claude's and Codex's own memory folders and
  previews them against this store. Only the Memory page shows the result.
- **The skill survey**, which walks the library and every tool's skill folder.
- **One `Brain::open`**, whose whole-file integrity check is worth paying once a
  session and not before there is a window.

Detection also runs one thread per descriptor. The work is waiting, not
computing, so waiting in sequence adds the answers up; `synapse status` and the
terminal dashboard get that too.

A page that has not looked yet has to say so. An empty tool list reads as *no
tools found*, which is a different and much worse statement than *not looked
yet* — hence `probed`, the "Looking for your tools…" line, and the dash in place
of a count.

One runtime, one worker, made on first use (`ui/runtime.rs`). A
`Runtime::new()` per call site builds a multi-threaded runtime — a worker per
core, spawned and joined again — and the window did that eight times before it
existed, for a handful of SQLite queries.

## The hooks

`statusline` runs on every turn of every session and `session` on every start,
so both are measured with a real payload on stdin — the shape a tool actually
sends:

| Command | Cost |
| --- | --- |
| `synapse version` (the process floor) | 9ms |
| `synapse statusline` | 11ms |
| `synapse session` | 13ms |
| `synapse status` | 13ms |

Two to four milliseconds of work over the cost of starting a process at all.
They read through `glance`, so no launch reads the whole database to check it.

Measure with stdin closed and you will see two seconds: that is `INPUTWAIT`,
the deliberate wait for a client that opened the pipe and has not written yet.
It is not the hook being slow, and a benchmark that forgets it is measuring its
own test harness.

## Doing this again

```sh
cargo build --release
# window, from a scratch store
SYNAPSE_DATA=/tmp/scratch/data SYNAPSE_HOME=$HOME ./target/release/synapse
# hooks, with the payload a tool sends
echo '{"cwd":"'$PWD'"}' | ./target/release/synapse-cli statusline
```

Never point a dev build at the real data directory: it migrates the store to
whatever schema that build carries, and every older Synapse on the machine then
refuses to open it.

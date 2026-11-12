# Agent backend

Synapse gives agent harnesses one local record of work: what ran, what changed,
what was reviewed, where it ran, and how to recover it. The native **Activity**
screen is the readable view. The CLI and JSONL API are the integration surfaces.

It does not replace a coding harness. A harness owns the model loop and tools;
Synapse keeps the durable state around that work.

## The run lifecycle

One task is one **run**. A run may have ordered steps, changed-file records,
artifacts, checkpoints, reviews, comments, evaluations, a workspace, and previews.

1. Create a run with the project identity, harness name, and structured input.
2. Claim it from a runner. Renew the lease while work continues.
3. Record steps and changes as they happen. Store large output as artifacts.
4. Create checkpoints before risky edits. A checkpoint saves only explicit paths.
5. Submit a review with checks, reasoning, and unresolved concerns. Comments may
   target the run, an event, step, artifact, change, or file and line.
6. Mark the run succeeded, failed, waiting, or cancelled.

The event ledger is useful for observation, not authority. Structured run records
are the source of truth.

## Start with the CLI

Create a run by sending its input on stdin:

```sh
printf '%s\n' '{"task":"update the parser"}' | \
  synapse backend run create --project github.com/acme/parser --harness mytool
```

Use the returned run id as `$RUN`. Record a changed file:

```sh
printf '%s\n' '{
  "path":"src/parser.rs",
  "operation":"modify",
  "summary":"Handle an empty token stream"
}' | synapse backend changes record "$RUN"
```

Submit a review handoff:

```sh
printf '%s\n' '{
  "reviewer":"reviewer",
  "verdict":"changes",
  "summary":"One regression case is missing",
  "checks":[{"name":"cargo test","status":"passed"}],
  "reasoning":"The empty input path is uncovered.",
  "concerns":["Add an empty-input test"]
}' | synapse backend review submit "$RUN"
```

Read it back without knowing the storage schema:

```sh
synapse backend changes list "$RUN"
synapse backend review list "$RUN"
synapse backend comment list "$RUN"
```

## Checkpoints

A checkpoint is deliberately explicit. Synapse does not sweep a repository or
capture ignored files. The harness supplies relative paths beneath one root:

```sh
printf '%s\n' '{
  "paths":["src/parser.rs","tests/parser.rs"],
  "label":"before parser rewrite",
  "state":{"phase":"editing"}
}' | synapse backend checkpoints create "$RUN" --root "$PWD"
```

Restore with the returned checkpoint id:

```sh
synapse backend checkpoints restore checkpoint-…
```

Restore replaces captured files, removes paths that were absent at capture time,
and restores run and step state. Credential-shaped filenames, text that requires
redaction, absolute paths, parent traversal, symlinks leaving the root, and more
than 1,000 files are refused.

## Isolated workspaces

Give a run a detached Git worktree:

```sh
synapse backend workspace create "$RUN" --root "$PWD"
```

The returned path is the worker's checkout. Accept its work explicitly:

```sh
synapse backend workspace merge "$RUN"
```

Merge checks the binary patch first, checkpoints every target path in the original
checkout, applies the patch, records the file changes, and marks the workspace
merged. It does not commit. Conflicting or credential-shaped changes are refused
before the original checkout is modified.

## Fork and move a run

Forking keeps the parent task and input while isolating another approach:

```sh
synapse backend run fork "$RUN" --title "Try the smaller parser change"
```

Export carries the run, steps, changes, reviews, comments, evaluations, and
attached artifacts. Worktree paths and checkpoint roots stay local:

```sh
synapse backend run export "$RUN" > run.json
synapse backend run import --project github.com/acme/parser < run.json
```

The import receives a new local id and keeps the source id in `replayof`.

## Runners and previews

A runner says which machine can claim work and what it can do. No hosted control
plane is required:

```sh
printf '%s\n' '{
  "name":"Studio Mac",
  "capabilities":["shell","git","preview"],
  "local":true
}' | synapse backend runner put studio

synapse backend runner heartbeat studio busy
```

A runner may attach a development URL to a run:

```sh
printf '%s\n' '{
  "id":"parser-preview",
  "runnerid":"studio",
  "url":"http://127.0.0.1:4000",
  "status":"ready"
}' | synapse backend preview put "$RUN"
```

Synapse records the URL; it does not create a tunnel or publish a local port. A
remote runner owns authentication and reachability.

## JSONL API

`synapse backend api` reads one request per line and writes one correlated response
per line. It opens no listener and exits at end of input.

```json
{"version":1,"id":"runs","method":"runs.list","params":{"limit":20}}
```

Method families mirror the CLI: `runs`, `steps`, `changes`, `reviews`, `comments`,
`artifacts`, `checkpoints`, `workspaces`, `runners`, `previews`, `events`,
`capabilities`, `triggers`, `routes`, and `evaluations`.

## Retention and trust

- Events retain 30 days and are capped at 50,000 rows.
- Runs retain 90 days and are capped at 10,000 rows. Active runs are not pruned.
- Artifacts are content-addressed and verified when read.
- Secret-shaped text is refused or redacted before structured state is written.
- Capabilities are default-deny and may be pinned to a content digest.
- A runner descriptor is not a trust grant. Project scopes still apply.

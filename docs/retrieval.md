# Retrieval checks

Recall finds relevant memories within the configured budget. `readmemory` reads the
exact stored body of one hit in bounded pages. Both apply the same global plus
current-project scope and exclude superseded records. Neither requires a model,
network service, or second index.

## Exact reading

Call `readmemory` with the id and project from a recall. Start at offset zero, then
pass each returned `next` as the next offset until it is null. Offsets and `total`
are UTF-8 byte counts. Concatenating the page bodies reconstructs stored text
without compaction, ellipses, or broken characters.

Lean limits a body page to 2,800 bytes. Balanced and Full limit it to 6,000 bytes.
These byte bounds also fit within the configured character budget. Source labels
have a separate 240-byte allowance with `sourceabridged` marking truncation;
metadata and JSON framing are additional to the body allowance. SQLite selects
only the requested body slice, so paging does not allocate the whole memory in
the server.

Missing, superseded, and out-of-scope ids all return a null memory. An offset
beyond a visible body's end or inside a UTF-8 character is an error. The exact end
returns an empty body and null `next`. Pages read current data; restart at zero if
the memory is edited between requests. Owner-facing CLI inspection and restore
remain available for hidden history.

The MCP tool is also available through bridges that discover the server's tool
list. Existing user guidance is preserved; the server supplies the reading hint
at initialization when it is absent.

## Correctness fixtures

Run from `crates/synapsecore`:

```sh
cargo test --test retrieval --test mcp
```

The retrieval fixtures check opposing statements, changed numbers and short
identifiers, exact duplicates, Unicode search, scope isolation, supersession and
restore, every budget combination, and lossless paging of long text containing
code, whitespace, multibyte characters, and embedded NULs. Protocol tests exercise
the complete recall-to-read flow through the released tool schema.

Approximate wording overlap cannot establish that two facts are equivalent.
Balanced and Lean retain exact-body deduplication but no longer suppress distinct
bodies by word-set similarity. Only explicit supersession establishes a correction.

## Performance fixture

```sh
cargo run --release --example retrieval -- 10000 30
cargo run --release --example retrieval -- 100000 30
```

The arguments are memory count and measured iterations, defaulting to 10,000 and
30. They are bounded at one million memories and one thousand iterations. The
fixture creates and removes its own temporary database; it never opens the user's
store, changes settings, or connects to a network service.

Output is JSON containing build profile, seed time, database-open time, database
bytes, and p50/p95/max latency plus structured response bytes for:

- selective recall: a term in every thousandth memory;
- common-term recall: a term present throughout the store;
- recent scoped recall: the newest visible memories;
- exact reading: the first bounded page of one memory.

The corpus is deterministic and all-global, with approximately 240-byte bodies.
Each operation has one untimed warmup. Timings include library calls and JSON
serialization, but exclude process startup, transport framing, model/client
processing, and fixture generation. The database-open measurement includes
verification after seeding, with OS file caches still warm. Response sizes count
structured JSON once; MCP transports may carry a text representation as well.

Use release builds and record the machine alongside results. Compare the same
fixture before and after a change. The fixture exposes scaling trends; it does
not simulate cold disks, mixed-project query plans, long-duration memory growth,
or semantic relevance on real user questions. Timing thresholds are deliberately
absent from shared-runner correctness tests. Before changing ranking or adding
inference, evaluate representative questions with expected relevant ids and measure
both recall quality and resource cost.

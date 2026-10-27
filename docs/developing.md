# Developing Synapse

The website, terminal bridge, core engine, native window, and optional sync server
have separate checks. Run the gates for every surface a change touches.

## Repository map

| Path | What lives here |
| --- | --- |
| `crates/synapsecore` | Library, CLI, terminal dashboard, MCP, memory, credentials, mesh, skills, setup, imports |
| `crates/synapse` | Native desktop window over core |
| `crates/synapsesync` | Shared encrypted sync wire types |
| `crates/synapseserve` | Optional opaque sync log server |
| `pi` | Terminal bridge package embedded by the core build |
| `website` | Static site templates, styles, browser behavior, and manual content |
| `docs` | Technical documentation, rendered into the public manual |
| `site` | Generated site, committed for GitHub Pages |

The root, core, and server are independent Cargo workspaces. Core and server must
resolve without the desktop dependency graph. The desktop pins its released UI
dependency exactly. Desktop and core versions must agree.

## Build and test

From the repository root:

```sh
cargo build --locked
cargo test --locked
cargo fmt --check
cargo test --locked --manifest-path crates/synapsecore/Cargo.toml
cargo check --locked --manifest-path crates/synapsecore/Cargo.toml --no-default-features
cargo test --locked --manifest-path crates/synapsecore/Cargo.toml --test cli --test mcp
cargo test --locked --manifest-path crates/synapseserve/Cargo.toml
```

For the terminal bridge:

```sh
cd pi
bun install
bun run check
```

See [Windows](windows.md) for the installer toolchain and platform smoke check.

## Work on the website

Use Bun 1.4.3 or later. Technical Markdown uses Bun's built-in renderer; the site
has no package dependency or runtime service. Build before starting the preview:

```sh
bun run site
bun run sitecheck
bun run serve
```

Open `http://127.0.0.1:4173/synapse/`. The prefix matters: GitHub Pages publishes
under `/synapse/`, and every local link and asset must work from nested routes.
Rebuild after source changes. `PORT` changes the preview port.

- `website/home.ts` owns the landing page.
- `website/template.ts` owns the shared frame, navigation, and article layout.
- `website/navigation.ts` groups the manual.
- `website/docs` and `website/tutorials` hold task and reference pages.
- Every Markdown file under `docs` becomes a searchable manual page automatically.
- `website/technical.ts` adds titles, source links, code controls, and contents.
- `website/deploy.ts` is the source for canonical and release URLs. The displayed
  release version comes from the desktop manifest.

The checker verifies local links and fragments, metadata, landmarks, navigation
coverage, command coverage, search indexing, and documentation sources. Inspect
both desktop and mobile in a browser as well: source checks cannot verify layout,
keyboard navigation, search interaction, or clipboard behavior.

Commit source and regenerated `site` together. The Pages workflow builds and
checks again on `main`, then publishes the artifact. Confirm the workflow and the
live page after pushing.

## Documentation boundaries

Document released behavior as released behavior. Label proposals and snapshots
explicitly. Do not turn a library module into a promised CLI setup flow, or a
performance fixture into a product-wide benchmark.

[The core library](synapsecore.md), [Embedding](embedding.md),
[Retrieval checks](retrieval.md), [The memory map](map.md),
[What launching costs](startup.md), and [Encrypted sync](sync.md) describe the
implementation and its checks. The [Federation proposal](federation.md) is a
design record and does not establish available commands.

## Contributions

Public command, MCP, descriptor, and storage surfaces are compatibility promises.
Preserve user-owned configuration and instructions. Managed writes need backups,
atomic replacement, and selective removal. Memory corrections retain a restore
path; proposed skills reach no tool before approval. Keep credential values out
of memory, scope YAML, protocol responses, logs, and command arguments.

For a tool connector, add a descriptor rather than extending a closed product
enum. Include detection, connection, repair, and selective cleanup checks. Update
the supported-tool table and the manual with the platforms actually tested.

For a scene plugin, keep the work in its own folder and use `synapse plugin
init`, `check`, and `install` for the local development loop. A plugin remains
declarative data: extend the versioned manifest and its validation before adding
a new capability, keep reads inside the plugin directory, and update
[Scenes and plugins](scenes.md) plus the CLI reference when the surface changes.

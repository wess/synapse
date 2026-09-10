# The memory map

The map is the store drawn as a graph: one node per memory, one per project, and
links between them. It answers what a list cannot — which projects this machine
holds anything about, where the weight is, and what a decision sits next to.

It is built by `synapsecore::brain::map`, which reads and lays out; both
dashboards draw the result. No model, no network service, no second index, and
no state of its own.

## What is on it

**Nodes.** One per memory, plus one hub per project and one for global memory.
A memory's node carries its opening line as a label, the hub it belongs to, and
how many links touch it. Superseded memories are on the map, drawn hollow: they
are out of recall, still in the store, and still restorable.

**Links.** Three kinds, and only one of them is inferred.

| Link | Meaning | Source |
| --- | --- | --- |
| Scope | This memory belongs to that project, or to global memory. | Recorded |
| Supersedes | This memory was replaced by that one. | Recorded |
| Shared | Both memories use enough of the same uncommon words. | Inferred |

A supersession is drawn only when both memories are on the map; a line to a node
that is not there is worse than no line. Shared links use the stoplist recall
uses, so a search and a map cannot disagree about which words mean nothing. Two
memories are linked when they share at least two words of four characters or
more, counting the first forty words of each; every memory keeps its three
strongest, and a pair is drawn once however many times it is chosen.

## Three dimensions

The layout is a volume, not a plane. On a plane a cluster with more neighbours
than the plane has room for either overlaps itself or shoves everything else off
the map, and both of those are the layout lying about the store; in a volume it
uses the third direction instead. The coordinates are the unit cube, y downward
and z toward the viewer.

The window turns it. Drag to turn, scroll to move closer or further out —
toward whatever the pointer is over, not the middle — and shift-drag to slide
the cloud in the frame. Nearer nodes are drawn larger and brighter, which is the
only depth cue a still frame has. Reset view puts the camera back where it
started.

The terminal draws the front of the same cloud, which is where the window's
camera starts: the same picture, before anybody turned it. Depth there decides
only what is drawn over what, since a cell holds one character. Past sixty
memories it leaves the inferred links out and says so in its title — the window
draws them at a tenth of the opacity of a recorded link, and a braille cell has
no tenth to spend.

## What it costs

Everything is bounded, because a page that costs a second to open is a page
nobody opens twice.

- 160 memories are read and laid out, newest first. Both surfaces read that
  constant rather than choosing one, so neither shows a different half of the
  same store. The page reports both numbers whenever the store is larger.
- Inferred links are capped per memory, so a store where everything resembles
  everything draws hundreds of lines rather than the twelve thousand a full
  pairing would.
- The layout is a fixed number of passes over a fixed number of nodes, seeded
  deterministically. The same store opens onto the same map every time: a map is
  worth having because its shape is learnable, and one that settled differently
  on a slower machine would be two maps.
- Turning the map re-projects and repaints; it never lays out again. The layout
  is the expensive half and it does not depend on where you are looking from.

Nothing is stored. The map is read when the page is opened and again when it is
refreshed, never held and redrawn from a store that has moved on.

## Reading it in either surface

The desktop paints the whole map into one canvas — quads for the nodes, stroked
paths for the links, through the same pipeline `guise::GpuView` submits to. It
is painted rather than laid out because a node's size, its opacity, and the
order it is drawn in all depend on where the camera is, and that is arithmetic
rather than layout. A click is answered by projecting the same points again and
taking the nearest circle the pointer landed in; it reads that memory by id and
opens it beside the map — by id, because the map covers the whole store while
the memory list covers one search.

The plot is dark whatever the theme is. A glow is light added to what is behind
it, and on a light background there is nothing to add to: every halo reads as a
smudge. The page around it stays the user's theme.

The terminal draws the same coordinates in braille. A terminal cannot hover, so
the cursor keys walk the nodes and whatever the cursor lands on is spelled out
underneath.

`SYNAPSE_PAGE=map` opens the desktop app straight onto it.

## Fixtures

Run from `crates/synapsecore`:

```sh
cargo test brain::graph
cargo test --lib tui
```

The graph fixtures check hub assignment and naming, that a supersession off the
map draws nothing, that shared words link related memories and common words link
nothing, the per-memory cap, label extraction, that a bounded map reports what it
left out, that every node lands inside the unit cube, that two runs place every
node identically, that the cloud has real depth rather than being a plane with a
third number attached, and that nodes are spread rather than stacked. The terminal
fixtures draw every page at sizes down to one cell by one, because a panic inside
a draw happens with the terminal already in raw mode.

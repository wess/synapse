//! The store as a map.
//!
//! Memory is a list everywhere else in Synapse, and a list is the right shape
//! for reading one memory and the wrong shape for seeing a store. It sorts by
//! age, which is the one relationship between two memories that carries no
//! meaning, and it says nothing about the thing a person actually wants to
//! know: which projects this machine holds anything about, where the weight
//! is, and what a decision made last week sits next to.
//!
//! So: nodes and links, laid out here rather than in either dashboard, and laid
//! out in three dimensions. A graph is not a flat thing: on a plane a cluster
//! with more neighbours than the plane has room for either overlaps itself or
//! shoves everything else off the map, and both of those are the layout lying
//! about the store. In a volume it uses the third direction instead.
//!
//! The window and the terminal draw the same map because they are handed the
//! same coordinates — a layout computed twice is two layouts that disagree
//! about the same machine by next year, and neither one is drawing. The window
//! turns the cloud; the terminal draws the front of it, which is where the
//! window's camera starts.
//!
//! Three kinds of link, and only one of them is inferred:
//!
//! - **Scope** ties a memory to its project, or to global memory. Recorded.
//! - **Supersedes** ties a memory to the one that replaced it. Recorded, and
//!   the reason a correction is visible as a correction rather than as two
//!   memories that happen to be about the same thing.
//! - **Shared** ties two memories that use enough of the same uncommon words.
//!   Inferred, drawn faintest, and never the only thing holding a node on the
//!   map.
//!
//! Everything here is bounded. [`NODES`] memories are read and no more, the
//! passes over them are a fixed count, and the inferred links are capped per
//! node — a map of a store this machine cannot draw is worth less than a map
//! of its newest corner, and a page that costs a second to open is a page
//! nobody opens twice.

use crate::brain::{Brain, Memory, MemoryScope};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

/// How many memories the map draws.
///
/// Both dashboards read this rather than choosing for themselves, so the two
/// cannot show different halves of the same store. Past a couple of hundred
/// nodes a force layout stops being a map and becomes a cloud, and the map
/// says how many it left out rather than pretending it drew everything.
pub const NODES: usize = 160;

/// One memory, or one thing memories belong to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A project, or global memory. Drawn larger, labelled always.
    Hub,
    Memory,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    /// The memory this stands for, or `None` for a hub. It is the id a surface
    /// selects by, so a click on the map and a row in the list mean the same
    /// thing.
    pub memory: Option<i64>,
    pub label: String,
    /// Which hub this node belongs to, and so which colour it takes. A hub is
    /// in its own cluster.
    pub cluster: usize,
    /// How many links touch it. Size comes from this: a memory nothing else
    /// refers to should look like one.
    pub weight: usize,
    /// Replaced by a newer memory, and so out of recall. Drawn hollow rather
    /// than hidden — it is still in the store, and still restorable.
    pub superseded: bool,
    /// Position in the unit cube, `0.0..=1.0`, y downward and z toward the
    /// viewer. A surface scales it into whatever box it has; one that draws
    /// flat uses `x` and `y` and reads `z` as depth, which is the same map
    /// seen from the front.
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tie {
    Scope,
    Supersedes,
    Shared,
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub tie: Tie,
}

#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    /// Memories in the store. `shown` of them are on the map, and the
    /// difference is the whole reason both numbers are here.
    pub held: i64,
    pub shown: usize,
    /// The names behind [`Node::cluster`], in cluster order.
    pub clusters: Vec<String>,
}

impl Graph {
    pub fn isempty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node standing for a memory id, if it is on the map.
    pub fn find(&self, memory: i64) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.memory == Some(memory))
    }
}

/// Read the newest [`NODES`] memories and lay them out.
///
/// One read, the same one for both surfaces. It takes superseded memories with
/// it — a correction that vanished from the map would leave the memory that
/// replaced it looking like it had always been there.
pub async fn map(brain: &Brain) -> Result<Graph> {
    let memories = brain.search("", NODES as u32).await?;
    let held = brain.stats().await.map(|stats| stats.entries)?;
    Ok(build(&memories, held))
}

/// The map for a set of memories, without a store to read it from.
///
/// Split out because it is the whole of the interesting part and none of the
/// I/O: everything about which nodes appear, what links them, and where they
/// land is decided here and can be asserted on directly.
pub fn build(memories: &[Memory], held: i64) -> Graph {
    let memories = &memories[..memories.len().min(NODES)];
    if memories.is_empty() {
        return Graph {
            held,
            ..Graph::default()
        };
    }

    // Hubs first, so a surface can draw them last and know they are the tail of
    // nothing — and so cluster and node index are settled before any link is
    // made.
    let mut clusters: Vec<String> = Vec::new();
    let mut hubof: BTreeMap<String, usize> = BTreeMap::new();
    for memory in memories {
        let key = hubkey(memory);
        if !hubof.contains_key(&key) {
            hubof.insert(key.clone(), clusters.len());
            clusters.push(hubname(memory));
        }
    }

    let mut nodes: Vec<Node> = clusters
        .iter()
        .enumerate()
        .map(|(cluster, name)| Node {
            kind: Kind::Hub,
            memory: None,
            label: name.clone(),
            cluster,
            weight: 0,
            superseded: false,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        })
        .collect();

    let hubs = nodes.len();
    let mut index: BTreeMap<i64, usize> = BTreeMap::new();
    for memory in memories {
        let cluster = hubof[&hubkey(memory)];
        index.insert(memory.id, nodes.len());
        nodes.push(Node {
            kind: Kind::Memory,
            memory: Some(memory.id),
            label: label(&memory.body),
            cluster,
            weight: 0,
            superseded: memory.superseded != 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        });
    }

    let mut links: Vec<Link> = Vec::new();
    for (offset, memory) in memories.iter().enumerate() {
        links.push(Link {
            from: hubof[&hubkey(memory)],
            to: hubs + offset,
            tie: Tie::Scope,
        });
    }
    // A supersession only draws when both ends are on the map. Half a link is a
    // line to a node that is not there.
    for memory in memories {
        if memory.superseded == 0 {
            continue;
        }
        if let (Some(from), Some(to)) = (
            index.get(&memory.id).copied(),
            index.get(&memory.superseded).copied(),
        ) {
            links.push(Link {
                from,
                to,
                tie: Tie::Supersedes,
            });
        }
    }
    links.extend(shared(memories, hubs));

    for link in &links {
        nodes[link.from].weight += 1;
        nodes[link.to].weight += 1;
    }
    place(&mut nodes, &links);

    Graph {
        shown: memories.len(),
        nodes,
        links,
        held,
        clusters,
    }
}

/// What a memory hangs off: its project, or global memory.
fn hubkey(memory: &Memory) -> String {
    match memory.scope {
        MemoryScope::Global => String::new(),
        MemoryScope::Project => memory.project.clone(),
    }
}

/// The hub's name, which is the project's folder rather than its path. A column
/// of absolute paths is a column of the same twelve leading characters.
fn hubname(memory: &Memory) -> String {
    match memory.scope {
        MemoryScope::Global => "Global".to_owned(),
        MemoryScope::Project => {
            let path = memory.project.trim_end_matches('/');
            match path.rsplit('/').next().filter(|name| !name.is_empty()) {
                Some(name) => name.to_owned(),
                None => "Project".to_owned(),
            }
        }
    }
}

/// How many uncommon words two memories must share before a link is drawn.
///
/// One is a coincidence — two memories about different things both mentioning
/// `release` — and a map where everything touches everything says as much as a
/// map where nothing does.
const OVERLAP: usize = 2;

/// The most inferred links one memory keeps. Its strongest, so a memory in a
/// dense corner of the store still lands beside the ones it is most like
/// rather than tying itself to the whole neighbourhood.
const NEIGHBOURS: usize = 3;

/// Words shorter than this carry too little to be evidence of anything: `main`
/// and `test` appear in half a store, and two-letter tokens appear in all of
/// it.
const SHORTEST: usize = 4;

/// Words taken from one memory. A long memory would otherwise link to
/// everything simply by mentioning more.
const VOCABULARY: usize = 40;

/// The inferred links: memories that use enough of the same uncommon words.
///
/// This is the only thing on the map nobody recorded, which is why it is capped
/// twice — at [`OVERLAP`] shared words before a pair counts at all, and at
/// [`NEIGHBOURS`] per memory afterwards. Both ends keep their strongest, and a
/// pair is drawn once however many times it is chosen.
fn shared(memories: &[Memory], offset: usize) -> Vec<Link> {
    let vocabularies: Vec<BTreeSet<&str>> = memories
        .iter()
        .map(|memory| vocabulary(&memory.body))
        .collect();
    let mut best: Vec<Vec<(usize, usize)>> = vec![Vec::new(); memories.len()];
    for left in 0..memories.len() {
        for right in (left + 1)..memories.len() {
            let count = vocabularies[left]
                .intersection(&vocabularies[right])
                .count();
            if count < OVERLAP {
                continue;
            }
            best[left].push((count, right));
            best[right].push((count, left));
        }
    }
    let mut chosen: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (from, mut candidates) in best.into_iter().enumerate() {
        // Strongest first, and the lower index first among equals, so the same
        // memories always produce the same map.
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, to) in candidates.into_iter().take(NEIGHBOURS) {
            chosen.insert((from.min(to), from.max(to)));
        }
    }
    chosen
        .into_iter()
        .map(|(from, to)| Link {
            from: offset + from,
            to: offset + to,
            tie: Tie::Shared,
        })
        .collect()
}

/// The words in one memory worth comparing against another's: long enough to
/// mean something, not on the stoplist recall already drops, and counted once
/// however often they appear.
fn vocabulary(body: &str) -> BTreeSet<&str> {
    body.split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= SHORTEST)
        .filter(|word| !crate::brain::store::STOPWORDS.contains(&word.to_lowercase().as_str()))
        .take(VOCABULARY)
        .collect()
}

/// How long a node's label is before it is cut. Long enough to recognise the
/// memory, short enough that a hundred of them are not a wall of text.
const LABEL: usize = 44;

/// A memory's opening words, as one line.
///
/// The first line that says something: a Markdown heading marker, a bullet, or
/// a fence is punctuation somebody typed and not what the memory is about.
fn label(body: &str) -> String {
    let line = body
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '-', '*', '>', '`'])
                .trim()
        })
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let words = line.split_whitespace().collect::<Vec<_>>().join(" ");
    match words.chars().count() > LABEL {
        true => format!("{}…", words.chars().take(LABEL - 1).collect::<String>()),
        false => words,
    }
}

/// How many times the layout is relaxed. A fixed count rather than a
/// convergence test: the same store has to produce the same map every time it
/// is opened, and a map that settles differently on a slower machine is two
/// maps.
const PASSES: usize = 90;

/// How far a node may move on the first pass, as a fraction of the cube. It
/// falls off by [`COOLING`] each pass, which is what turns a scramble into a
/// layout instead of a permanent wobble.
const HEAT: f32 = 0.10;
const COOLING: f32 = 0.955;

/// The pull toward the middle. Without it a memory that shares nothing with
/// anything is pushed off the map by everything else, and the map is mostly
/// empty space.
const GRAVITY: f32 = 0.012;

/// A hub is what its memories hang off, so it is heavier than they are and
/// moves less. Otherwise the projects drift and their memories follow, which
/// reads as one cloud rather than several.
const ANCHOR: f32 = 0.45;

/// Force-directed placement in three dimensions, seeded deterministically and
/// run for a fixed number of passes.
///
/// Three rather than two because a graph is not a flat thing. On a plane, a
/// cluster with more neighbours than the plane has room for either overlaps
/// itself or pushes everything else off the map; in a volume it simply uses
/// the third direction, which is why the same store reads as separate
/// projects here and as one smear when it was flattened.
///
/// A random seed would put the same store in a different place every time it
/// was opened, and a map somebody has learned the shape of is worth more than
/// a marginally prettier one they have not.
fn place(nodes: &mut [Node], links: &[Link]) {
    let count = nodes.len();
    if count == 1 {
        nodes[0].x = 0.5;
        nodes[0].y = 0.5;
        nodes[0].z = 0.5;
        return;
    }

    // A Fibonacci spiral over the sphere, pulled inward by a cube root so the
    // scatter fills the volume rather than coating its shell. Successive points
    // land in the gaps left by the ones before, which is an even start without
    // being a pattern anybody can see in the result.
    const GOLDEN: f32 = 2.399_963_2;
    for (index, node) in nodes.iter_mut().enumerate() {
        let fraction = (index as f32 + 0.5) / count as f32;
        let polar = (1.0 - 2.0 * fraction).clamp(-1.0, 1.0).acos();
        let azimuth = index as f32 * GOLDEN;
        let radius = fraction.cbrt();
        node.x = radius * polar.sin() * azimuth.cos();
        node.y = radius * polar.sin() * azimuth.sin();
        node.z = radius * polar.cos();
    }

    let ideal = (1.0 / count as f32).cbrt();
    let mut heat = HEAT;
    let mut shift = vec![[0.0f32; 3]; count];
    for _ in 0..PASSES {
        shift.iter_mut().for_each(|value| *value = [0.0; 3]);

        for left in 0..count {
            for right in (left + 1)..count {
                let delta = [
                    nodes[left].x - nodes[right].x,
                    nodes[left].y - nodes[right].y,
                    nodes[left].z - nodes[right].z,
                ];
                let distance = length(delta).max(1e-4);
                let force = ideal * ideal / distance;
                for axis in 0..3 {
                    let push = delta[axis] / distance * force;
                    shift[left][axis] += push;
                    shift[right][axis] -= push;
                }
            }
        }

        for link in links {
            let delta = [
                nodes[link.from].x - nodes[link.to].x,
                nodes[link.from].y - nodes[link.to].y,
                nodes[link.from].z - nodes[link.to].z,
            ];
            let distance = length(delta).max(1e-4);
            // A recorded link pulls harder than an inferred one, so what the
            // store actually knows decides the shape and the resemblances only
            // decorate it.
            let strength = match link.tie {
                Tie::Scope => 1.0,
                Tie::Supersedes => 0.8,
                Tie::Shared => 0.45,
            };
            let force = distance * distance / ideal * strength;
            for axis in 0..3 {
                let pull = delta[axis] / distance * force;
                shift[link.from][axis] -= pull;
                shift[link.to][axis] += pull;
            }
        }

        for (index, node) in nodes.iter_mut().enumerate() {
            shift[index][0] -= node.x * GRAVITY / ideal;
            shift[index][1] -= node.y * GRAVITY / ideal;
            shift[index][2] -= node.z * GRAVITY / ideal;
            let distance = length(shift[index]).max(1e-4);
            let step = distance.min(heat) / distance;
            let weight = match node.kind {
                Kind::Hub => ANCHOR,
                Kind::Memory => 1.0,
            };
            node.x += shift[index][0] * step * weight;
            node.y += shift[index][1] * step * weight;
            node.z += shift[index][2] * step * weight;
        }
        heat *= COOLING;
    }

    normalise(nodes);
}

fn length(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

/// The margin left around the map, so a node on the edge is not half a node.
const MARGIN: f32 = 0.06;

/// Fit the layout into the unit cube, one scale for all three axes.
///
/// Scaling each axis to fill would stretch a cloud that came out tall into a
/// cube, which moves every node relative to every other. What the layout
/// decided is a shape; this only moves and sizes it.
fn normalise(nodes: &mut [Node]) {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    for node in nodes.iter() {
        for (axis, value) in [node.x, node.y, node.z].into_iter().enumerate() {
            low[axis] = low[axis].min(value);
            high[axis] = high[axis].max(value);
        }
    }
    let span = (0..3)
        .map(|axis| high[axis] - low[axis])
        .fold(1e-4f32, f32::max);
    let scale = (1.0 - 2.0 * MARGIN) / span;
    // Centre the shorter axes rather than pinning them to the margin.
    let pad = |axis: usize| (1.0 - (high[axis] - low[axis]) * scale) / 2.0;
    let (padx, pady, padz) = (pad(0), pad(1), pad(2));
    for node in nodes.iter_mut() {
        node.x = ((node.x - low[0]) * scale + padx).clamp(0.0, 1.0);
        node.y = ((node.y - low[1]) * scale + pady).clamp(0.0, 1.0);
        node.z = ((node.z - low[2]) * scale + padz).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(id: i64, body: &str, project: &str) -> Memory {
        Memory {
            id,
            body: body.to_owned(),
            source: "test".to_owned(),
            scope: match project.is_empty() {
                true => MemoryScope::Global,
                false => MemoryScope::Project,
            },
            project: project.to_owned(),
            created: 1_700_000_000 + id,
            superseded: 0,
            abridged: false,
        }
    }

    #[test]
    fn an_empty_store_maps_to_an_empty_graph() {
        let graph = build(&[], 0);
        assert!(graph.isempty());
        assert_eq!(graph.shown, 0);
        assert!(graph.links.is_empty());
    }

    /// Every memory hangs off exactly one hub, and the hub is the project's
    /// folder rather than the path nobody can read a column of.
    #[test]
    fn every_memory_hangs_off_its_project() {
        let graph = build(
            &[
                memory(1, "deploys run from the release branch", "/src/synapse"),
                memory(2, "the vault holds no values", "/src/synapse"),
                memory(3, "prefer small modules", ""),
            ],
            3,
        );
        assert_eq!(
            graph.clusters,
            vec!["synapse".to_owned(), "Global".to_owned()]
        );
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.shown, 3);
        let scopes = graph
            .links
            .iter()
            .filter(|link| link.tie == Tie::Scope)
            .count();
        assert_eq!(scopes, 3);
        for node in graph.nodes.iter().filter(|node| node.memory.is_some()) {
            assert!(node.weight >= 1, "{} is on the map alone", node.label);
        }
    }

    /// A correction is a link, because the alternative is two memories about
    /// the same thing with nothing saying which one won.
    #[test]
    fn a_correction_links_to_what_replaced_it() {
        let mut old = memory(1, "the release is cut from a tag", "/src/synapse");
        old.superseded = 2;
        let graph = build(
            &[
                old,
                memory(2, "the release is cut from main", "/src/synapse"),
            ],
            2,
        );
        let link = graph
            .links
            .iter()
            .find(|link| link.tie == Tie::Supersedes)
            .expect("a supersession link");
        assert_eq!(graph.nodes[link.from].memory, Some(1));
        assert_eq!(graph.nodes[link.to].memory, Some(2));
        assert!(graph.nodes[link.from].superseded);
        assert!(!graph.nodes[link.to].superseded);
    }

    /// A supersession pointing at a memory too old to be on the map draws
    /// nothing, rather than a line to a node that is not there.
    #[test]
    fn a_supersession_off_the_map_draws_no_link() {
        let mut only = memory(1, "the release is cut from a tag", "/src/synapse");
        only.superseded = 900;
        let graph = build(&[only], 1);
        assert!(!graph.links.iter().any(|link| link.tie == Tie::Supersedes));
    }

    /// Two memories about the same subject are drawn beside each other, and one
    /// about something else is not tied to them by the words everything uses.
    #[test]
    fn memories_sharing_uncommon_words_are_linked_and_others_are_not() {
        let graph = build(
            &[
                memory(1, "the vault keeps credentials in the keychain", ""),
                memory(
                    2,
                    "credentials in the keychain are never in the response",
                    "",
                ),
                memory(3, "terminals redraw the whole frame every tick", ""),
            ],
            3,
        );
        let shared: Vec<(Option<i64>, Option<i64>)> = graph
            .links
            .iter()
            .filter(|link| link.tie == Tie::Shared)
            .map(|link| (graph.nodes[link.from].memory, graph.nodes[link.to].memory))
            .collect();
        assert_eq!(shared, vec![(Some(1), Some(2))]);
    }

    /// The stoplist recall drops is the stoplist the map drops, or a page of
    /// memories all containing `that` reads as a page about one subject.
    #[test]
    fn common_words_are_not_evidence_of_anything() {
        let graph = build(
            &[
                memory(1, "that would have been what they were", ""),
                memory(2, "there could have been which they should", ""),
            ],
            2,
        );
        assert!(!graph.links.iter().any(|link| link.tie == Tie::Shared));
    }

    /// One memory keeps its strongest few resemblances. Without the cap, a
    /// store where everything resembles everything draws every pair — 12,720
    /// lines at a full map — and a map that dense is a filled square.
    ///
    /// Ten memories saying the same thing is the worst case: each keeps its
    /// [`NEIGHBOURS`] and no pair is drawn twice, so the count is bounded by
    /// the store and not by its density.
    #[test]
    fn one_memory_keeps_only_its_strongest_resemblances() {
        let bodies: Vec<Memory> = (1..=10)
            .map(|id| {
                memory(
                    id,
                    "release notarize staple homebrew signing pipeline artefacts",
                    "",
                )
            })
            .collect();
        let graph = build(&bodies, 10);
        let inferred = graph
            .links
            .iter()
            .filter(|link| link.tie == Tie::Shared)
            .count();
        assert!(inferred <= bodies.len() * NEIGHBOURS, "{inferred} links");
        // Every pair would be 45 of them.
        assert!(inferred < 45, "{inferred} links");
    }

    #[test]
    fn a_label_is_the_opening_words_without_the_punctuation() {
        assert_eq!(label("# Heading\n\nthe body"), "Heading");
        assert_eq!(label("- a bullet point"), "a bullet point");
        assert_eq!(label("\n\n  spaced   out  \n"), "spaced out");
        assert_eq!(label(""), "");
        let long = label(&"alpha ".repeat(40));
        assert_eq!(long.chars().count(), LABEL);
        assert!(long.ends_with('…'));
    }

    /// A store bigger than the map says so rather than drawing what it can and
    /// leaving somebody to assume that is all of it.
    #[test]
    fn a_store_larger_than_the_map_reports_both_numbers() {
        let memories: Vec<Memory> = (1..=(NODES as i64 + 40))
            .map(|id| memory(id, &format!("memory number {id}"), ""))
            .collect();
        let graph = build(&memories, memories.len() as i64);
        assert_eq!(graph.shown, NODES);
        assert_eq!(graph.held, NODES as i64 + 40);
    }

    /// Every node lands inside the cube, or a surface draws half of one off
    /// the edge of its own box.
    #[test]
    fn every_node_lands_inside_the_unit_cube() {
        let memories: Vec<Memory> = (1..=60)
            .map(|id| {
                memory(
                    id,
                    &format!("memory {id} about vaults scopes and credentials"),
                    match id % 3 {
                        0 => "/src/synapse",
                        1 => "/src/guise",
                        _ => "",
                    },
                )
            })
            .collect();
        let graph = build(&memories, 60);
        for node in &graph.nodes {
            assert!((0.0..=1.0).contains(&node.x), "{} x {}", node.label, node.x);
            assert!((0.0..=1.0).contains(&node.y), "{} y {}", node.label, node.y);
            assert!((0.0..=1.0).contains(&node.z), "{} z {}", node.label, node.z);
        }
        // A layout that collapsed onto a plane would be a 2D map with a third
        // number attached, and turning it would show nothing.
        let (low, high) = graph
            .nodes
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), node| {
                (low.min(node.z), high.max(node.z))
            });
        assert!(
            high - low > 0.5,
            "the cloud is flat: z spans {}",
            high - low
        );
    }

    /// The same store opens onto the same map. A layout seeded from a clock or
    /// an address would move every node between one launch and the next, and a
    /// map is worth having because its shape is learnable.
    #[test]
    fn the_same_store_lays_out_the_same_way_every_time() {
        let memories: Vec<Memory> = (1..=30)
            .map(|id| memory(id, &format!("memory {id} about releases and vaults"), ""))
            .collect();
        let once = build(&memories, 30);
        let again = build(&memories, 30);
        for (left, right) in once.nodes.iter().zip(again.nodes.iter()) {
            assert_eq!(left.x.to_bits(), right.x.to_bits(), "{}", left.label);
            assert_eq!(left.y.to_bits(), right.y.to_bits(), "{}", left.label);
            assert_eq!(left.z.to_bits(), right.z.to_bits(), "{}", left.label);
        }
    }

    /// Two nodes on top of each other are one node as far as anybody looking at
    /// the map is concerned.
    #[test]
    fn nodes_are_spread_rather_than_stacked() {
        let memories: Vec<Memory> = (1..=40)
            .map(|id| memory(id, &format!("unrelated memory number {id}"), ""))
            .collect();
        let graph = build(&memories, 40);
        let mut closest = f32::MAX;
        for left in 0..graph.nodes.len() {
            for right in (left + 1)..graph.nodes.len() {
                closest = closest.min(length([
                    graph.nodes[left].x - graph.nodes[right].x,
                    graph.nodes[left].y - graph.nodes[right].y,
                    graph.nodes[left].z - graph.nodes[right].z,
                ]));
            }
        }
        assert!(closest > 0.01, "two nodes {closest} apart");
    }

    #[test]
    fn a_memory_is_found_by_its_id_and_a_missing_one_is_not() {
        let graph = build(&[memory(7, "the only memory", "")], 1);
        let found = graph.find(7).expect("the memory on the map");
        assert_eq!(graph.nodes[found].memory, Some(7));
        assert!(graph.find(8).is_none());
    }
}

//! The store as a map, and a way to move around inside it.
//!
//! Every other page here is a list, and a list is the wrong shape for the
//! question people actually ask a memory store: not *what is in it* but *what
//! is it about*. Projects come out as clusters, a correction comes out as a
//! line to the thing it corrected, and a memory nothing else touches comes out
//! looking like one — which is usually either the most interesting node on the
//! map or the one that should not be there.
//!
//! Nothing about the *map* is decided here. The nodes, the links, and their
//! coordinates in the unit cube all come from [`synapsecore::brain::map`], so
//! the window and the terminal draw the same machine; this file is the camera
//! and the paint.
//!
//! **Why one canvas rather than elements.** The first version of this page laid
//! nodes out as absolutely positioned `div`s over a canvas of links, which made
//! clicking free. In three dimensions that stops working: a node's size and
//! opacity depend on how far away it is, the draw order has to be back to
//! front, and a glow is several overlapping circles rather than one box. All of
//! that is per-frame arithmetic, and layout is not where arithmetic goes. So
//! everything is painted — through `paint_quad` and `paint_path`, the same
//! pipeline `guise::GpuView` submits to — and the click is answered by
//! [`pick`], which projects the same points the paint did and takes the nearest
//! circle the pointer landed in. `GpuScene` itself is quads and textures with
//! no rotation, so the links, which are most of what makes this look like
//! anything, cannot come from it.
//!
//! **Why it is dark whatever the theme is.** A glow is light added to what is
//! behind it. On a light background there is nothing to add to: every halo
//! reads as a smudge and the whole thing turns grey. The plot is a viewport
//! onto something, the way a photograph is dark in a bright room, and the page
//! around it stays the user's theme.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClickEvent, Corners, Edges, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, Window, canvas, div, point, px, quad, size,
};
use guise::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use synapsecore::brain::{Graph, Memory, MemoryScope, Node, NodeKind, Tie};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
/// The mouse, in the four shapes this page has to answer. Named rather than
/// written out, because a map is the one page here where every one of them
/// means something and four boxed closures of the same size in a row is four
/// chances to wire one to the wrong verb.
pub type Press = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
pub type Drag = Box<dyn Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static>;
pub type Release = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>;
pub type Zoom = Box<dyn Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static>;
/// Opening one node, by the id the map carries for it.
pub type Select = Box<dyn Fn(i64) -> Click>;

/// Where the map is being looked at from.
///
/// Two angles, a zoom, and an offset — no camera position, because there is
/// nowhere to go: the cloud is the only thing in the world and it is always in
/// the middle of it. Turning it is the whole of the navigation, which is also
/// why the state is four numbers a page can hand back and forth rather than a
/// matrix somebody has to keep orthonormal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: (f32, f32),
}

impl Default for Camera {
    /// Facing the front of the cloud, which is the picture the terminal draws.
    /// Opening the window and running `synapse` in a terminal beside it should
    /// not look like two different stores.
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

/// How far the pitch may go, just short of the pole. At the pole the yaw axis
/// and the view axis line up and turning left does nothing, which reads as the
/// map having jammed.
const LIMIT: f32 = 1.45;

/// Radians per pixel dragged. Slow enough to aim, fast enough that a look at
/// the other side is a flick and not a chore.
const TURN: f32 = 0.008;

impl Camera {
    pub fn turn(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * TURN;
        self.pitch = (self.pitch + dy * TURN).clamp(-LIMIT, LIMIT);
    }

    /// Slide the cloud within the viewport. In fractions of the shorter side,
    /// so a pan means the same thing whatever the window is doing.
    pub fn shift(&mut self, dx: f32, dy: f32, span: f32) {
        self.pan.0 += dx / span.max(1.0);
        self.pan.1 += dy / span.max(1.0);
    }

    /// A wheel notch, toward whatever the pointer is over.
    ///
    /// Multiplicative, because a fixed step is a crawl when you are far out and
    /// a jump when you are close in. `toward` is the pointer's offset from the
    /// middle of the viewport in the same units the pan is kept in, and the pan
    /// is moved so that whatever was under it stays under it — zooming into the
    /// middle when you were looking at the edge is how you lose the thing you
    /// were reaching for.
    pub fn scale(&mut self, delta: f32, toward: (f32, f32)) {
        let before = self.zoom;
        self.zoom = (self.zoom * (1.0 + delta * 0.0016)).clamp(0.35, 6.0);
        let ratio = self.zoom / before;
        self.pan.0 = toward.0 - (toward.0 - self.pan.0) * ratio;
        self.pan.1 = toward.1 - (toward.1 - self.pan.1) * ratio;
    }
}

pub struct View {
    pub graph: Graph,
    /// The memory whose node was last clicked, read from the store rather than
    /// from the list on the memories page: the map is the whole store and that
    /// list is whatever was searched for.
    pub selected: Option<Memory>,
    pub camera: Camera,
    /// Where the plot was laid out, filled in as it is painted.
    ///
    /// A click arrives in window coordinates and has to be turned back into a
    /// node, which needs the box the projection used. Nothing else on this page
    /// knows that box: it is decided by the layout, after the page is built.
    pub viewport: Rc<Cell<Option<Bounds<Pixels>>>>,
}

pub struct Actions {
    pub select: Select,
    pub press: Press,
    pub drag: Drag,
    pub release: Release,
    pub zoom: Zoom,
    pub home: Click,
    pub refresh: Click,
    pub open: Click,
}

/// One colour per project.
///
/// Six, because past that colour stops telling two projects apart and the label
/// is doing the work anyway. They are lit rather than pigmented — high value,
/// modest saturation — because every one of them is drawn as light on a near
/// black field and a dark colour there is a hole. `tui/theme.rs` carries the
/// same six in the form a terminal can take, so the same project is the same
/// colour in either surface.
const CLUSTERS: [(u8, u8, u8); 6] = [
    (124, 196, 255),
    (110, 240, 214),
    (186, 152, 255),
    (255, 196, 120),
    (255, 150, 205),
    (168, 245, 152),
];

/// The field the map is drawn on. Not black: a little blue keeps the halos from
/// reading as grey, and it is the colour the page's own dark theme is heading
/// toward anyway.
const FIELD: (u8, u8, u8) = (8, 10, 18);

/// How many memories a map can name before the labels are the map.
const CROWDED: usize = 60;

/// How far the camera sits from the middle of the cloud, and the focal length
/// it looks through, both in cube widths.
///
/// The ratio is the whole of the perspective: at this pair a node at the back
/// is about two thirds the size of the same node at the front, which is enough
/// to read as depth and not so much that the far half of the store becomes
/// unreadable.
const DISTANCE: f32 = 2.4;
const FOCAL: f32 = 2.1;

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let Actions {
        select,
        press,
        drag,
        release,
        zoom,
        home,
        refresh,
        open,
    } = actions;

    if view.graph.isempty() {
        return div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .child(Icon::new(IconName::ChartNetwork).size(Size::Lg))
            .child(Title::new("Nothing to map yet").order(4))
            .child(
                Text::new("Connect a tool and memory arrives as your sessions run.")
                    .size(Size::Sm)
                    .dimmed(),
            )
            .child(
                Button::new("mapconnect", "Connections")
                    .variant(Variant::Light)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .on_click(move |event, window, cx| open(event, window, cx)),
            )
            .into_any_element();
    }

    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .child(header(&view.graph, view.camera, home, refresh, cx))
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .child(plot(
                    &view,
                    Handlers {
                        select,
                        press,
                        drag,
                        release,
                        zoom,
                    },
                ))
                .child(rail(&view, cx)),
        )
        .into_any_element()
}

struct Handlers {
    select: Select,
    press: Press,
    drag: Drag,
    release: Release,
    zoom: Zoom,
}

fn header(
    graph: &Graph,
    camera: Camera,
    home: Click,
    refresh: Click,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let counted = match graph.shown as i64 == graph.held {
        true => format!("{} memories", graph.held),
        // The map is bounded, so it says what it left out rather than letting
        // its edge read as the edge of the store.
        false => format!("{} of {} memories, newest first", graph.shown, graph.held),
    };
    div()
        .flex_none()
        .px(px(24.0))
        .py(px(14.0))
        .border_b_1()
        .border_color(theme.border().hsla())
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(Title::new("Map").order(4))
                .child(Text::new(counted).size(Size::Xs).dimmed()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .children(
                    graph
                        .clusters
                        .iter()
                        .enumerate()
                        .take(CLUSTERS.len())
                        .map(|(index, name)| key(cluster(index), name.clone())),
                )
                // Only once it has been turned. A control that undoes nothing
                // is a control that has to be read before it can be ignored.
                .when(camera != Camera::default(), |element| {
                    element.child(
                        Button::new("maphome", "Reset view")
                            .variant(Variant::Subtle)
                            .size(Size::Sm)
                            .on_click(move |event, window, cx| home(event, window, cx)),
                    )
                })
                .child(
                    Button::new("maprefresh", "Refresh")
                        .variant(Variant::Light)
                        .size(Size::Sm)
                        .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                        .on_click(move |event, window, cx| refresh(event, window, cx)),
                ),
        )
}

/// One entry of the legend: the colour, and what it stands for.
fn key(colour: Hsla, label: String) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(div().size(px(8.0)).rounded_full().bg(colour))
        .child(Text::new(label).size(Size::Xs).dimmed())
}

fn cluster(index: usize) -> Hsla {
    let (red, green, blue) = CLUSTERS[index % CLUSTERS.len()];
    guise::rgb(red, green, blue)
}

fn alpha(mut colour: Hsla, value: f32) -> Hsla {
    colour.a = value;
    colour
}

/// The map itself: one painted surface, and the mouse.
fn plot(view: &View, handlers: Handlers) -> impl IntoElement {
    let Handlers {
        select,
        press,
        drag,
        release,
        zoom,
    } = handlers;
    let graph = view.graph.clone();
    let camera = view.camera;
    let viewport = view.viewport.clone();
    let open = view.selected.as_ref().map(|memory| memory.id);
    let crowded = graph.shown > CROWDED;

    // A click and a turn arrive on the same button, the way they do in every
    // other map: what tells them apart is whether the pointer moved, and only
    // the caller has somewhere to remember that.
    let picker = view.viewport.clone();
    let picked = graph.clone();
    div()
        .id("mapplot")
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .relative()
        .overflow_hidden()
        // An open hand, because the first thing this surface does is move.
        .cursor(gpui::CursorStyle::OpenHand)
        .bg(guise::rgb(FIELD.0, FIELD.1, FIELD.2))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            press(event, window, cx)
        })
        .on_mouse_move(move |event, window, cx| drag(event, window, cx))
        .on_mouse_up(MouseButton::Left, move |event, window, cx| {
            release(event, window, cx)
        })
        .on_scroll_wheel(move |event, window, cx| zoom(event, window, cx))
        .on_click(move |event, window, cx| {
            let Some(bounds) = picker.get() else {
                return;
            };
            if let Some(id) = pick(&picked, camera, bounds, event.position()) {
                select(id)(event, window, cx);
            }
        })
        .child(
            canvas(
                move |bounds, _, _| {
                    // Where the projection happened, kept for the next click.
                    viewport.set(Some(bounds));
                },
                move |bounds, _, window, cx| {
                    paint(&graph, camera, open, crowded, bounds, window, cx);
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// One node, as the camera sees it.
struct Placed {
    at: Point<Pixels>,
    /// How much bigger or smaller than at the middle of the cloud. Also the
    /// depth cue: nearer is larger.
    scale: f32,
    /// Distance from the camera, for sorting and for fading the back out.
    depth: f32,
}

/// Turn, tilt, and project one point of the unit cube into the viewport.
fn place(node: &Node, camera: Camera, bounds: Bounds<Pixels>) -> Placed {
    let span = span(bounds);
    let (x, y, z) = (node.x - 0.5, node.y - 0.5, node.z - 0.5);

    let (sinyaw, cosyaw) = camera.yaw.sin_cos();
    let (turnedx, turnedz) = (x * cosyaw + z * sinyaw, z * cosyaw - x * sinyaw);
    let (sinpitch, cospitch) = camera.pitch.sin_cos();
    let (tiltedy, depth) = (
        y * cospitch - turnedz * sinpitch,
        y * sinpitch + turnedz * cospitch,
    );

    // Behind the camera is not a place a node can be — the cloud is one unit
    // wide and the camera is more than two away — but the clamp is what keeps a
    // hand-set zoom from ever dividing by nothing.
    let away = (DISTANCE - depth).max(0.4);
    let scale = FOCAL / away * camera.zoom;
    Placed {
        at: point(
            bounds.origin.x + bounds.size.width / 2.0 + px((turnedx * scale + camera.pan.0) * span),
            bounds.origin.y
                + bounds.size.height / 2.0
                + px((tiltedy * scale + camera.pan.1) * span),
        ),
        scale: scale / (FOCAL / DISTANCE),
        depth: away,
    }
}

/// How big a node is drawn before perspective, by how much is tied to it.
///
/// Square-rooted: the difference between one link and four is worth seeing, the
/// difference between forty and fifty is not, and the busiest node on a map of
/// two hundred must not be the size of the page.
fn radius(node: &Node) -> f32 {
    let base = match node.kind {
        NodeKind::Hub => 5.0,
        NodeKind::Memory => 2.5,
    };
    base + (node.weight as f32).sqrt() * 0.8
}

/// The node the pointer is over, nearest first.
///
/// The same projection the paint used, run again for one point. Running it
/// again rather than remembering where every node landed is the cheaper of the
/// two: it is a hundred and sixty multiplications on a click, against a vector
/// kept in step with every frame of every drag.
pub fn pick(
    graph: &Graph,
    camera: Camera,
    bounds: Bounds<Pixels>,
    at: Point<Pixels>,
) -> Option<i64> {
    let mut best: Option<(f32, i64)> = None;
    for node in &graph.nodes {
        let Some(id) = node.memory else {
            continue;
        };
        let placed = place(node, camera, bounds);
        // Generous, because a four-pixel circle is not a four-pixel target.
        let reach = (radius(node) * placed.scale).max(7.0);
        let (dx, dy) = (f32::from(placed.at.x - at.x), f32::from(placed.at.y - at.y));
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > reach {
            continue;
        }
        // Nearest to the camera wins, not nearest to the pointer: what is in
        // front is what somebody thinks they are clicking.
        if best.is_none_or(|(depth, _)| placed.depth < depth) {
            best = Some((placed.depth, id));
        }
    }
    best.map(|(_, id)| id)
}

/// How dark the far side of the cloud goes. Nothing disappears — a node you
/// cannot see is a node you cannot find by turning toward it.
fn fade(depth: f32) -> f32 {
    (1.25 - (depth - (DISTANCE - 0.5)) * 0.55).clamp(0.30, 1.0)
}

fn paint(
    graph: &Graph,
    camera: Camera,
    open: Option<i64>,
    crowded: bool,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let placed: Vec<Placed> = graph
        .nodes
        .iter()
        .map(|node| place(node, camera, bounds))
        .collect();

    links(graph, &placed, window);

    // Back to front, so a node in front covers the one behind it and its glow
    // lands on top of what it should be lighting.
    let mut order: Vec<usize> = (0..graph.nodes.len()).collect();
    order.sort_by(|left, right| placed[*right].depth.total_cmp(&placed[*left].depth));
    for index in &order {
        node(&graph.nodes[*index], &placed[*index], open, window);
    }
    for index in &order {
        let node = &graph.nodes[*index];
        let named = node.kind == NodeKind::Hub || node.memory == open || !crowded;
        if named && !node.label.is_empty() {
            label(node, &placed[*index], bounds, window, cx);
        }
    }
}

/// Every link, in as few tessellations as the colouring allows.
///
/// One path per colour rather than one per link: a stroked path is tessellated
/// on the CPU, and four hundred of those on every frame of a drag is the
/// difference between turning the map and watching it turn.
fn links(graph: &Graph, placed: &[Placed], window: &mut Window) {
    let mut groups: Vec<(Hsla, f32, Vec<usize>)> = Vec::new();
    for (index, link) in graph.links.iter().enumerate() {
        let (colour, width) = match link.tie {
            Tie::Scope => (alpha(cluster(graph.nodes[link.from].cluster), 0.42), 1.0),
            // The one thing on the map nobody inferred, and the one thing that
            // has to survive being drawn under everything else.
            Tie::Supersedes => (guise::rgba(255, 120, 140, 0.75), 1.8),
            Tie::Shared => (guise::rgba(150, 180, 220, 0.18), 0.9),
        };
        match groups
            .iter_mut()
            .find(|(existing, size, _)| *existing == colour && *size == width)
        {
            Some((_, _, members)) => members.push(index),
            None => groups.push((colour, width, vec![index])),
        }
    }

    for (colour, width, members) in groups {
        // Twice: a wide dim pass for the light around the line, a thin bright
        // one for the line. It is the cheapest bloom there is and the only one
        // available without a shader of our own.
        for (weight, strength) in [(width * 3.0, 0.28), (width, 1.0)] {
            let mut builder = PathBuilder::stroke(px(weight));
            let mut drew = false;
            for index in &members {
                let link = &graph.links[*index];
                builder.move_to(placed[link.from].at);
                builder.line_to(placed[link.to].at);
                drew = true;
            }
            // Building an empty path is an error, and a map with no
            // corrections on it is the ordinary case.
            if drew && let Ok(path) = builder.build() {
                window.paint_path(path, alpha(colour, colour.a * strength));
            }
        }
    }
}

fn node(node: &Node, placed: &Placed, open: Option<i64>, window: &mut Window) {
    let colour = cluster(node.cluster);
    let lit = fade(placed.depth);
    let across = (radius(node) * placed.scale).max(1.0);
    let chosen = node.memory.is_some() && node.memory == open;

    let circle = |centre: Point<Pixels>, across: f32| Bounds {
        origin: point(centre.x - px(across), centre.y - px(across)),
        size: size(px(across * 2.0), px(across * 2.0)),
    };
    let disc = |bounds: Bounds<Pixels>, colour: Hsla| {
        quad(
            bounds,
            Corners::all(bounds.size.width / 2.0),
            colour,
            Edges::all(px(0.0)),
            gpui::transparent_black(),
            BorderStyle::Solid,
        )
    };

    // Three circles of falling opacity. A real bloom is a blur pass; this is
    // what a blur of a dot looks like from a distance, and it costs three
    // quads.
    window.paint_quad(disc(
        circle(placed.at, across * 4.2),
        alpha(colour, 0.045 * lit),
    ));
    window.paint_quad(disc(
        circle(placed.at, across * 2.1),
        alpha(colour, 0.12 * lit),
    ));

    match node.superseded {
        // Out of recall and still in the store: a ring rather than a body, and
        // never simply absent.
        true => window.paint_quad(quad(
            circle(placed.at, across),
            Corners::all(px(across)),
            gpui::transparent_black(),
            Edges::all(px(1.6)),
            alpha(colour, 0.85 * lit),
            BorderStyle::Solid,
        )),
        false => {
            window.paint_quad(disc(circle(placed.at, across), alpha(colour, 0.92 * lit)));
            // The hot middle. Every light source in the reference has one, and
            // without it a node is a flat sticker rather than something lit.
            window.paint_quad(disc(
                circle(placed.at, across * 0.38),
                alpha(guise::rgb(255, 255, 255), 0.6 * lit),
            ));
        }
    }

    if chosen {
        window.paint_quad(quad(
            circle(placed.at, across + 5.0),
            Corners::all(px(across + 5.0)),
            gpui::transparent_black(),
            Edges::all(px(1.4)),
            alpha(guise::rgb(255, 255, 255), 0.9),
            BorderStyle::Solid,
        ));
    }
}

/// A node's name, beside it.
///
/// Shaped and painted here rather than laid out as an element: where it goes is
/// decided by the camera, and the camera moves between one frame and the next.
fn label(node: &Node, placed: &Placed, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let size = match node.kind {
        NodeKind::Hub => 12.0,
        NodeKind::Memory => 10.5,
    };
    let colour = match node.kind {
        NodeKind::Hub => alpha(guise::rgb(255, 255, 255), 0.92 * fade(placed.depth)),
        NodeKind::Memory => alpha(cluster(node.cluster), 0.72 * fade(placed.depth)),
    };
    let run = gpui::TextRun {
        len: node.label.len(),
        font: window.text_style().font(),
        color: colour,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(node.label.clone().into(), px(size), &[run], None);
    // Off the right-hand edge the name is written on the other side, which is
    // the only reason this needs to know how wide it came out.
    let gap = px(radius(node) * placed.scale + 7.0);
    let left = placed.at.x + gap;
    let at = match left + line.width > bounds.origin.x + bounds.size.width - px(8.0) {
        true => point(
            placed.at.x - gap - line.width,
            placed.at.y - px(size * 0.65),
        ),
        false => point(left, placed.at.y - px(size * 0.65)),
    };
    let _ = line.paint(at, px(size * 1.4), window, cx);
}

/// What the last click landed on, and how to read the map when nothing has been
/// clicked yet.
fn rail(view: &View, cx: &App) -> impl IntoElement {
    let theme = guise::theme(cx);
    let column = div()
        .flex_none()
        .w(px(316.0))
        .h_full()
        .border_l_1()
        .border_color(theme.border().hsla())
        .bg(theme.surface().hsla())
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(12.0));

    let Some(memory) = view.selected.clone() else {
        return column
            .child(Text::new("Reading the map").size(Size::Sm).bold())
            .child(
                Text::new(
                    "Each point is one memory, sized by how much it is tied to and coloured by \
                     the project it belongs to. Click one to read it.",
                )
                .size(Size::Xs)
                .dimmed(),
            )
            .child(div().h(px(4.0)))
            .child(legend(theme.dimmed().hsla(), theme.danger().hsla()))
            .child(div().h(px(4.0)))
            .child(Text::new("Moving around").size(Size::Sm).bold())
            .child(hint("Drag", "turn the cloud"))
            .child(hint("Scroll", "closer or further out"))
            .child(hint("⇧ drag", "slide it in the frame"));
    };

    let scope = match memory.scope {
        MemoryScope::Global => "global".to_owned(),
        MemoryScope::Project => memory.project.clone(),
    };
    let source = match memory.source.is_empty() {
        true => "no source".to_owned(),
        false => memory.source.clone(),
    };
    column
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    Text::new(format!("Memory #{}", memory.id))
                        .size(Size::Sm)
                        .bold(),
                )
                .child(Text::new(stamp(memory.created)).size(Size::Xs).dimmed()),
        )
        .child(Text::new(scope).size(Size::Xs).dimmed())
        .child(Text::new(source).size(Size::Xs).dimmed())
        .when(memory.superseded != 0, |element| {
            element.child(
                Text::new(format!(
                    "Replaced by #{}, and no longer recalled.",
                    memory.superseded
                ))
                .size(Size::Xs)
                .color(theme.danger()),
            )
        })
        .child(
            div()
                .id("mapbody")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(Text::new(memory.body.clone()).size(Size::Xs)),
        )
}

fn hint(key: &'static str, what: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .flex_none()
                .w(px(52.0))
                .child(Text::new(key).size(Size::Xs)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(Text::new(what).size(Size::Xs).dimmed()),
        )
}

fn legend(border: Hsla, danger: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.0))
        // The same three weights the map draws, so the key is a sample of it
        // rather than a description of one.
        .child(line(
            alpha(border, 0.85),
            "a memory and the project it belongs to",
        ))
        .child(line(
            alpha(border, 0.5),
            "two memories using the same uncommon words",
        ))
        .child(line(danger, "a memory and the one that replaced it"))
        .child(
            Text::new("A hollow point is a superseded memory: still stored, no longer recalled.")
                .size(Size::Xs)
                .dimmed(),
        )
}

fn line(colour: Hsla, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .flex_none()
                .w(px(18.0))
                .h(px(3.0))
                .rounded(px(2.0))
                .bg(colour),
        )
        // A flex child will not wrap below its longest word unless it is told
        // it may shrink, and the label is wider than the rail.
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(Text::new(label).size(Size::Xs).dimmed()),
        )
}

fn stamp(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

/// The shorter side of a viewport, which is the unit a pan and a zoom are kept
/// in so that both mean the same thing whatever the window is doing.
pub fn span(bounds: Bounds<Pixels>) -> f32 {
    f32::from(bounds.size.width).min(f32::from(bounds.size.height))
}

/// Where a point sits relative to the middle of the viewport, in spans.
pub fn offset(bounds: Bounds<Pixels>, at: Point<Pixels>) -> (f32, f32) {
    let span = span(bounds).max(1.0);
    (
        f32::from(at.x - bounds.origin.x - bounds.size.width / 2.0) / span,
        f32::from(at.y - bounds.origin.y - bounds.size.height / 2.0) / span,
    )
}

/// A wheel notch, whichever way the platform reports it.
pub fn notch(delta: ScrollDelta) -> f32 {
    match delta {
        ScrollDelta::Pixels(amount) => f32::from(amount.y),
        // A line is not a pixel, and a trackpad and a wheel land in different
        // arms of this; the multiplier is what keeps one notch of a mouse from
        // being a tenth of what a trackpad does.
        ScrollDelta::Lines(amount) => amount.y * 24.0,
    }
}

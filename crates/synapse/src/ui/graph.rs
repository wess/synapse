//! The store as a map.
//!
//! Every other page here is a list, and a list is the wrong shape for the
//! question people actually ask a memory store: not *what is in it* but *what
//! is it about*. Projects come out as clusters, a correction comes out as a
//! line to the thing it corrected, and a memory nothing else touches comes out
//! looking like one — which is usually either the most interesting node on the
//! map or the one that should not be there.
//!
//! Nothing about the map is decided here. The nodes, the links, and the
//! coordinates all come from [`synapsecore::brain::map`], so the window and the
//! terminal draw the same picture of the same machine; this file is the paint.
//!
//! Links are painted into one canvas and nodes are laid over it as ordinary
//! elements. A node has to be clickable and a line does not, and a page that
//! paints everything itself would have to do its own hit testing to find out
//! which circle a click landed in.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Hsla, PathBuilder, Pixels, Window, canvas, div, point, px,
    relative,
};
use guise::prelude::*;
use synapsecore::brain::{Graph, Memory, MemoryScope, NodeKind, Tie};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub graph: Graph,
    /// The memory whose node was last clicked, read from the store rather than
    /// from the list on the memories page: the map is the whole store and that
    /// list is whatever was searched for.
    pub selected: Option<Memory>,
}

pub struct Actions {
    pub select: Box<dyn Fn(i64) -> Click>,
    pub refresh: Click,
    pub open: Click,
}

/// One colour per project.
///
/// Six, because past that colour stops telling two projects apart and the
/// label is doing the work anyway. `tui/theme.rs` carries the same six for the
/// same reason it carries the rest of this palette: the same project should be
/// the same colour in either surface.
const CLUSTERS: [(u8, u8, u8); 6] = [
    (154, 132, 230),
    (83, 190, 162),
    (226, 168, 96),
    (118, 178, 232),
    (214, 134, 196),
    (150, 196, 120),
];

fn cluster(index: usize) -> Hsla {
    let (red, green, blue) = CLUSTERS[index % CLUSTERS.len()];
    guise::rgb(red, green, blue)
}

/// How big a node is drawn, by how much is tied to it.
///
/// A memory nothing refers to has to look like one, and the busiest node on a
/// map of two hundred must not be the size of the page. Square-rooted for that
/// reason: the difference between one link and four is worth seeing, the
/// difference between forty and fifty is not.
fn diameter(weight: usize, kind: &NodeKind) -> f32 {
    let base = match kind {
        NodeKind::Hub => 20.0,
        NodeKind::Memory => 9.0,
    };
    base + (weight as f32).sqrt() * 3.4
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let Actions {
        select,
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
            .child(Icon::new(IconName::Waypoints).size(Size::Lg))
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
        .child(header(&view.graph, refresh, cx))
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .child(plot(
                    &view.graph,
                    view.selected.as_ref().map(|memory| memory.id),
                    select,
                    cx,
                ))
                .child(rail(&view, cx)),
        )
        .into_any_element()
}

fn header(graph: &Graph, refresh: Click, cx: &App) -> impl IntoElement {
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

/// The map itself: one canvas of links, and a node over it for each thing on
/// the map.
fn plot(
    graph: &Graph,
    open: Option<i64>,
    select: Box<dyn Fn(i64) -> Click>,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let faint = alpha(theme.border().hsla(), 0.55);
    let fainter = alpha(theme.border().hsla(), 0.30);
    let correction = alpha(theme.danger().hsla(), 0.55);
    let body = theme.body().hsla();
    let (text, dim) = (theme.text(), theme.dimmed());

    let links: Vec<([f32; 4], Tie)> = graph
        .links
        .iter()
        .map(|link| {
            let (from, to) = (&graph.nodes[link.from], &graph.nodes[link.to]);
            ([from.x, from.y, to.x, to.y], link.tie)
        })
        .collect();

    let nodes = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let size = diameter(node.weight, &node.kind);
            let colour = cluster(node.cluster);
            // A memory that has been replaced is out of recall and still in the
            // store, so it is drawn as an outline rather than left off the map.
            let fill = match node.superseded {
                true => body,
                false => colour,
            };
            // The one that is open is named whatever its size, because the
            // rail beside it is showing a memory and nothing on the map says
            // which one.
            let chosen = node.memory.is_some() && node.memory == open;
            let named = node.kind == NodeKind::Hub || node.weight >= 5 || chosen;
            // A label on a node near the right edge runs off the map, so the
            // ones over there are written on the other side. Anchoring by
            // `right` is what makes the row grow leftward rather than off it.
            let leftward = node.x > 0.66;
            let element = div()
                .absolute()
                .top(relative(node.y))
                // The position is the node's centre, and an edge is what is
                // being placed. Half the node back on each axis is what puts
                // the middle where the layout put it.
                .mt(px(-size / 2.0))
                .map(|element| match leftward {
                    true => element.right(relative(1.0 - node.x)).mr(px(-size / 2.0)),
                    false => element.left(relative(node.x)).ml(px(-size / 2.0)),
                })
                .flex()
                .when(leftward, |element| element.flex_row_reverse())
                .items_center()
                .gap(px(5.0))
                .child(
                    div()
                        .flex_none()
                        .size(px(size))
                        .rounded_full()
                        .bg(fill)
                        .border_2()
                        .border_color(match chosen {
                            true => text.hsla(),
                            false => colour,
                        }),
                )
                .when(named, |element| {
                    element.child(
                        div()
                            .flex_none()
                            .max_w(px(180.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(Text::new(node.label.clone()).size(Size::Xs).color(
                                match node.kind {
                                    NodeKind::Hub => text,
                                    NodeKind::Memory => match chosen {
                                        true => text,
                                        false => dim,
                                    },
                                },
                            )),
                    )
                });
            match node.memory {
                // A hub is a heading rather than a thing to open: everything it
                // stands for is already on the map around it.
                None => element.into_any_element(),
                Some(id) => {
                    let go = select(id);
                    element
                        .id(("mapnode", index))
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.75))
                        .on_click(move |event, window, cx| go(event, window, cx))
                        .into_any_element()
                }
            }
        })
        .collect::<Vec<_>>();

    div()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .relative()
        .overflow_hidden()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    // Three passes, faintest first, so a correction is never
                    // hidden under a resemblance that happens to cross it.
                    for (wanted, colour, width) in [
                        (Tie::Shared, fainter, 1.0),
                        (Tie::Scope, faint, 1.0),
                        (Tie::Supersedes, correction, 1.6),
                    ] {
                        let mut builder = PathBuilder::stroke(px(width));
                        let mut drew = false;
                        for ([x1, y1, x2, y2], tie) in &links {
                            if *tie != wanted {
                                continue;
                            }
                            builder.move_to(at(&bounds, *x1, *y1));
                            builder.line_to(at(&bounds, *x2, *y2));
                            drew = true;
                        }
                        // Building an empty path is an error, and a map with no
                        // corrections on it is the ordinary case.
                        if drew && let Ok(path) = builder.build() {
                            window.paint_path(path, colour);
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .children(nodes)
}

/// A unit-square coordinate, in the box the canvas was given.
fn at(bounds: &gpui::Bounds<Pixels>, x: f32, y: f32) -> gpui::Point<Pixels> {
    point(
        bounds.origin.x + bounds.size.width * x,
        bounds.origin.y + bounds.size.height * y,
    )
}

fn alpha(mut colour: Hsla, value: f32) -> Hsla {
    colour.a = value;
    colour
}

/// What the last click landed on, and how to read the map when nothing has
/// been clicked yet.
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
                    "Each circle is one memory, sized by how much it is tied to and coloured by \
                     the project it belongs to. Click one to read it.",
                )
                .size(Size::Xs)
                .dimmed(),
            )
            .child(div().h(px(4.0)))
            .child(legend(theme.border().hsla(), theme.danger().hsla()));
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
            Text::new("A hollow circle is a superseded memory: still stored, no longer recalled.")
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

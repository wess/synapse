//! The map, in a terminal.
//!
//! The same graph the window draws, from the same coordinates — braille cells
//! instead of paths, and labels only where there is room for one. A terminal
//! cannot hover, so the cursor keys walk the nodes and whatever it lands on is
//! spelled out underneath: on a map, the thing you are pointing at is the one
//! thing a picture cannot tell you.

use crate::brain::{NodeKind, Tie};
use crate::tui::state::{self, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as Edge};
use ratatui::widgets::{Padding, Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let rows = Layout::vertical([Constraint::Min(5), Constraint::Length(7)]).split(area);
    map(frame, rows[0], state);
    detail(frame, rows[1], state);
}

fn map(frame: &mut Frame, area: Rect, state: &State) {
    let graph = &state.graph;
    let title = match graph.shown as i64 == graph.held {
        true => format!("Map · {} memories", graph.held),
        false => format!("Map · {} of {} memories", graph.shown, graph.held),
    };
    if graph.isempty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  Nothing to map yet.", theme::dim())),
                Line::from(Span::styled(
                    "  Connect a tool and memory arrives as sessions run.",
                    theme::dim(),
                )),
            ])
            .block(draw::panel(&title)),
            area,
        );
        return;
    }
    let cursor = state::cursor(state);
    let canvas = Canvas::default()
        .block(draw::panel(&title))
        .marker(Marker::Braille)
        .x_bounds([0.0, 1.0])
        .y_bounds([0.0, 1.0])
        .paint(move |context| paint(context, state, cursor));
    frame.render_widget(canvas, area);
}

fn paint(context: &mut Context<'_>, state: &State, cursor: usize) {
    let graph = &state.graph;
    for link in &graph.links {
        let (from, to) = (&graph.nodes[link.from], &graph.nodes[link.to]);
        // What the store recorded is drawn in the accent; what the map inferred
        // from shared words is drawn in the border colour, because it is the
        // one thing here nobody wrote down.
        let colour = match link.tie {
            Tie::Scope => theme::CLUSTERS[from.cluster % theme::CLUSTERS.len()],
            Tie::Supersedes => theme::DANGER,
            Tie::Shared => theme::BORDER,
        };
        context.draw(&Edge {
            x1: from.x as f64,
            y1: flip(from.y),
            x2: to.x as f64,
            y2: flip(to.y),
            color: colour,
        });
    }
    // A layer of its own, so a node is never drawn under a line that happens to
    // pass through it.
    context.layer();
    for (index, node) in graph.nodes.iter().enumerate() {
        let colour = theme::CLUSTERS[node.cluster % theme::CLUSTERS.len()];
        let mark = match (index == cursor, &node.kind, node.superseded) {
            (true, _, _) => "◉",
            (_, NodeKind::Hub, _) => "◆",
            // Out of recall and still in the store: hollow rather than absent.
            (_, NodeKind::Memory, true) => "○",
            (_, NodeKind::Memory, false) => "●",
        };
        let style = match index == cursor {
            true => theme::selected(),
            false => Style::default().fg(colour),
        };
        context.print(node.x as f64, flip(node.y), Span::styled(mark, style));
    }
    context.layer();
    // Hubs are named, and so is whatever the cursor is on. Labelling every node
    // in a terminal writes the map over itself.
    for (index, node) in graph.nodes.iter().enumerate() {
        if node.kind != NodeKind::Hub && index != cursor {
            continue;
        }
        let style = match index == cursor {
            true => theme::accent(),
            false => Style::default().fg(theme::CLUSTERS[node.cluster % theme::CLUSTERS.len()]),
        };
        context.print(
            node.x as f64,
            flip(node.y),
            Span::styled(format!("  {}", shorten(&node.label, 28)), style),
        );
    }
}

/// The graph counts y downward, the way a screen does. A canvas counts it
/// upward, the way a chart does.
fn flip(y: f32) -> f64 {
    1.0 - y as f64
}

fn shorten(label: &str, limit: usize) -> String {
    match label.chars().count() > limit {
        true => format!("{}…", label.chars().take(limit - 1).collect::<String>()),
        false => label.to_owned(),
    }
}

/// What the cursor is on, spelled out. A hub says how much hangs off it; a
/// memory says what it is.
fn detail(frame: &mut Frame, area: Rect, state: &State) {
    let graph = &state.graph;
    let Some(node) = graph.nodes.get(state::cursor(state)) else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(" Nothing selected.", theme::dim())))
                .block(draw::panel("Selected").padding(Padding::horizontal(1))),
            area,
        );
        return;
    };
    let cluster = graph
        .clusters
        .get(node.cluster)
        .cloned()
        .unwrap_or_default();
    let title = match node.memory {
        Some(id) => format!("Memory #{id}"),
        None => node.label.clone(),
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{cluster} "), theme::accent()),
        Span::styled(
            format!(
                "· {} {}",
                node.weight,
                match node.weight {
                    1 => "link",
                    _ => "links",
                }
            ),
            theme::dim(),
        ),
    ])];
    match node.memory {
        None => lines.push(Line::from(Span::styled(
            "Everything stored under this heading hangs off here.",
            theme::dim(),
        ))),
        // The map is the whole store and the list beside it is whatever was
        // searched for, so a node the search left out has a label and no body.
        // Saying that is better than showing a label as though it were one.
        Some(id) => match memory(state, id) {
            Some(memory) => {
                if memory.superseded != 0 {
                    lines.push(Line::from(Span::styled(
                        format!("replaced by #{}, and no longer recalled", memory.superseded),
                        theme::bad(),
                    )));
                }
                lines.extend(
                    memory
                        .body
                        .lines()
                        .map(|line| Line::from(Span::styled(line.to_owned(), theme::text()))),
                );
            }
            None => {
                lines.push(Line::from(Span::styled(node.label.clone(), theme::text())));
                lines.push(Line::from(Span::styled(
                    "Outside the current search — clear it to read this one.",
                    theme::dim(),
                )));
            }
        },
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel(&title).padding(Padding::horizontal(1)))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The memory a node stands for, out of the list the memories page already
/// loaded. A map does not open the store again to say what it is pointing at.
fn memory(state: &State, id: i64) -> Option<&crate::brain::Memory> {
    state.memories.iter().find(|memory| memory.id == id)
}

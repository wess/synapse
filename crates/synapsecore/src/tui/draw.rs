//! The frame around the pages.
//!
//! Header, tabs, body, notice, keys — the desktop's shape, which is one console
//! rather than a wall of cards. The body is whatever the current page draws;
//! everything else on screen is drawn here and stays put, so moving between
//! pages never moves the furniture.

use crate::tui::state::{self, Mode, Notice, PAGES, Page, State};
use crate::tui::{connections, memories, mesh, settings, skills, theme, vaults};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub fn frame(frame: &mut Frame, state: &State) {
    let areas = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // tabs
        Constraint::Min(3),    // page
        Constraint::Length(1), // notice
        Constraint::Length(1), // keys
    ])
    .split(frame.area());

    title(frame, areas[0], state);
    tabs(frame, areas[1], state);
    body(frame, areas[2], state);
    notice(frame, areas[3], state);
    keys(frame, areas[4], state);

    if state.mode == Mode::Help {
        help(frame, frame.area());
    }
}

fn title(frame: &mut Frame, area: Rect, state: &State) {
    let count = state.stats.entries;
    let line = Line::from(vec![
        Span::styled(" Synapse", theme::heading()),
        Span::styled("  ·  ", theme::dim()),
        Span::styled(
            format!("{count} {}", if count == 1 { "memory" } else { "memories" }),
            theme::text(),
        ),
        Span::styled("  ·  ", theme::dim()),
        Span::styled(kilobytes(state.stats.bytes), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn kilobytes(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

fn tabs(frame: &mut Frame, area: Rect, state: &State) {
    let mut spans = vec![Span::raw(" ")];
    for (index, page) in PAGES.iter().enumerate() {
        let label = format!(" {} {} ", index + 1, state::title(*page));
        spans.push(if *page == state.page {
            Span::styled(label, theme::selected())
        } else {
            Span::styled(label, theme::dim())
        });
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn body(frame: &mut Frame, area: Rect, state: &State) {
    match state.page {
        Page::Connections => connections::draw(frame, area, state),
        Page::Memories => memories::draw(frame, area, state),
        Page::Mesh => mesh::draw(frame, area, state),
        Page::Skills => skills::draw(frame, area, state),
        Page::Vaults => vaults::draw(frame, area, state),
        Page::Settings => settings::draw(frame, area, state),
    }
}

fn notice(frame: &mut Frame, area: Rect, state: &State) {
    let style = match state.notice {
        Notice::Ready => theme::dim(),
        Notice::Success(_) => theme::good(),
        Notice::Error(_) => theme::bad(),
    };
    let mark = match state.notice {
        Notice::Ready => " ",
        Notice::Success(_) => " ✓ ",
        Notice::Error(_) => " ! ",
    };
    let line = Line::from(vec![
        Span::styled(mark, style),
        Span::styled(state.notice.message(), style),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn keys(frame: &mut Frame, area: Rect, state: &State) {
    let hints: &[(&str, &str)] = match state.mode {
        Mode::Search => &[("type", "filter"), ("enter", "apply"), ("esc", "done")],
        Mode::Naming => &[("type", "name"), ("enter", "describe"), ("esc", "cancel")],
        Mode::Confirm(_) => &[("y", "confirm"), ("any", "cancel")],
        Mode::Help => &[("any", "close")],
        Mode::Browse => match state.page {
            Page::Memories => &[
                ("↹", "page"),
                ("jk", "move"),
                ("/", "search"),
                ("d", "delete"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Page::Connections => &[
                ("↹", "page"),
                ("jk", "move"),
                ("c", "connect"),
                ("e", "describe"),
                ("d", "disconnect"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Page::Skills => &[
                ("↹", "page"),
                ("jk", "move"),
                ("a", "approve"),
                ("d", "turn down"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Page::Settings => &[
                ("↹", "page"),
                ("f/b/n", "budget"),
                ("m", "mesh"),
                ("s", "learn"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
            _ => &[
                ("↹", "page"),
                ("jk", "move"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
        },
    };
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in hints {
        spans.push(Span::styled(*key, theme::accent()));
        spans.push(Span::styled(format!(" {label}   "), theme::dim()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn help(frame: &mut Frame, area: Rect) {
    let width = 52.min(area.width.saturating_sub(4));
    let height = 17.min(area.height.saturating_sub(2));
    let box_ = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let lines = vec![
        Line::from(Span::styled("  Keys", theme::heading())),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  1-6, tab, ←→   ", theme::accent()),
            Span::raw("move between pages"),
        ]),
        Line::from(vec![
            Span::styled("  j k ↑↓         ", theme::accent()),
            Span::raw("move the cursor"),
        ]),
        Line::from(vec![
            Span::styled("  g G            ", theme::accent()),
            Span::raw("first and last row"),
        ]),
        Line::from(vec![
            Span::styled("  /              ", theme::accent()),
            Span::raw("search memories"),
        ]),
        Line::from(vec![
            Span::styled("  d              ", theme::accent()),
            Span::raw("delete the selected memory"),
        ]),
        Line::from(vec![
            Span::styled("  f b n          ", theme::accent()),
            Span::raw("full, balanced, lean recall"),
        ]),
        Line::from(vec![
            Span::styled("  m s            ", theme::accent()),
            Span::raw("mesh, and agent-written skills"),
        ]),
        Line::from(vec![
            Span::styled("  a              ", theme::accent()),
            Span::raw("approve a skill waiting for review"),
        ]),
        Line::from(vec![
            Span::styled("  r              ", theme::accent()),
            Span::raw("reload from the store"),
        ]),
        Line::from(vec![
            Span::styled("  q              ", theme::accent()),
            Span::raw("quit"),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  Secret values are never shown here.",
            theme::dim(),
        )),
    ];
    frame.render_widget(Clear, box_);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme::border()),
            )
            .wrap(Wrap { trim: false }),
        box_,
    );
}

/// A bordered panel with a title, which is what every page is made of.
pub fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(format!(" {title} "), theme::heading()))
}

/// One row of a list, styled for whether the cursor is on it.
pub fn row<'a>(spans: Vec<Span<'a>>, selected: bool) -> Line<'a> {
    if selected {
        Line::from(spans).style(theme::selected())
    } else {
        Line::from(spans)
    }
}

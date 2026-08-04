//! The memory list and whatever the cursor is on.
//!
//! Split the way the desktop splits it: the list narrow on the left, the whole
//! body on the right, because a memory is written to be read in full and a
//! truncated one is the same shape as a wrong one.

use crate::brain::MemoryScope;
use crate::tui::state::{self, Mode, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(area);
    search(frame, rows[0], state);
    let columns =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(rows[1]);
    list(frame, columns[0], state);
    detail(frame, columns[1], state);
}

fn search(frame: &mut Frame, area: Rect, state: &State) {
    let typing = state.mode == Mode::Search;
    let shown = if state.query.is_empty() && !typing {
        Span::styled("everything, most recent first", theme::dim())
    } else {
        Span::styled(state.query.clone(), theme::text())
    };
    let mut spans = vec![Span::styled(" ", theme::text()), shown];
    if typing {
        // A block rather than a real cursor: the terminal's cursor is parked
        // off-screen while the dashboard owns the display.
        spans.push(Span::styled("▌", theme::accent()));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(draw::panel(if typing {
            "Search · typing"
        } else {
            "Search · press /"
        })),
        area,
    );
}

fn list(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = state::cursor(state);
    // Keep the cursor on screen without a scroll offset in the state: the list
    // is the only thing that scrolls, so the window can be derived each frame.
    let height = area.height.saturating_sub(2) as usize;
    let first = cursor.saturating_sub(height.saturating_sub(1));
    let mut lines = Vec::new();
    if state.memories.is_empty() {
        lines.push(Line::from(Span::styled(
            if state.query.is_empty() {
                " Nothing stored yet."
            } else {
                " Nothing matches."
            },
            theme::dim(),
        )));
    }
    for (index, memory) in state.memories.iter().enumerate().skip(first).take(height) {
        let scope = match memory.scope {
            MemoryScope::Global => "global",
            MemoryScope::Project => "project",
        };
        let body = memory.body.lines().next().unwrap_or_default();
        lines.push(draw::row(
            vec![
                Span::styled(format!(" {:>5} ", memory.id), theme::dim()),
                Span::styled(format!("{scope:<8}"), theme::accent()),
                Span::styled(body.to_owned(), theme::text()),
            ],
            index == cursor,
        ));
    }
    let title = format!("Memories · {}", state.memories.len());
    frame.render_widget(Paragraph::new(lines).block(draw::panel(&title)), area);
}

fn detail(frame: &mut Frame, area: Rect, state: &State) {
    let Some(memory) = state::selectedmemory(state) else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(" Nothing selected.", theme::dim())))
                .block(draw::panel("Memory")),
            area,
        );
        return;
    };
    let scope = match memory.scope {
        MemoryScope::Global => "global".to_owned(),
        MemoryScope::Project => format!("project · {}", memory.project),
    };
    let source = if memory.source.is_empty() {
        "no source".to_owned()
    } else {
        memory.source.clone()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("scope   ", theme::dim()),
            Span::styled(scope, theme::accent()),
        ]),
        Line::from(vec![
            Span::styled("source  ", theme::dim()),
            Span::styled(source, theme::text()),
        ]),
        Line::from(vec![
            Span::styled("stored  ", theme::dim()),
            Span::styled(stamp(memory.created), theme::text()),
        ]),
        Line::raw(""),
    ];
    lines.extend(
        memory
            .body
            .lines()
            .map(|line| Line::from(Span::styled(line.to_owned(), theme::text()))),
    );
    // Padding rather than a leading space on each line: a body long enough to
    // wrap gets its continuation indented too, and a space would not.
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel(&format!("Memory #{}", memory.id)).padding(Padding::horizontal(1)))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn stamp(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|value| value.naive_local().format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| seconds.to_string())
}

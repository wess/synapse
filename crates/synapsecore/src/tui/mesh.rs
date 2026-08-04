//! Who is on the mesh and what they said they were doing.
//!
//! The roster sorts people first and the desktop marks them, so this does too.
//! A person is addressed, never assigned, and a screen that shows them as one
//! more worker is the screen that gets that wrong.

use crate::tui::state::{self, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    if !state.meshenabled {
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  The mesh is off.", theme::text())),
                Line::raw(""),
                Line::from(Span::styled(
                    "  Turning it on adds the coordination tools to every connected tool:",
                    theme::dim(),
                )),
                Line::from(Span::styled(
                    "  agents can message each other, hand work back and forth, and wait",
                    theme::dim(),
                )),
                Line::from(Span::styled(
                    "  for free between tasks. They cost context in each session, so the",
                    theme::dim(),
                )),
                Line::from(Span::styled(
                    "  mesh stays off until you want it.",
                    theme::dim(),
                )),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Settings", theme::accent()),
                    Span::styled(" · press m to turn it on", theme::dim()),
                ]),
            ])
            .block(draw::panel("Mesh"))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    if let Some(error) = &state.mesherror {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {error}"), theme::bad())))
                .block(draw::panel("Mesh"))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let areas = Layout::vertical([Constraint::Min(3), Constraint::Percentage(40)]).split(area);
    roster(frame, areas[0], state);
    workers(frame, areas[1], state);
}

fn roster(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = state::cursor(state);
    let mut lines = Vec::new();
    if state.agents.is_empty() {
        lines.push(Line::from(Span::styled(
            " Nobody has joined yet.",
            theme::dim(),
        )));
    }
    for (index, agent) in state.agents.iter().enumerate() {
        let (mark, style) = if agent.human {
            ("◆", theme::accent())
        } else if agent.online {
            (theme::mark(true), theme::good())
        } else {
            (theme::mark(false), theme::dim())
        };
        let status = if agent.status.is_empty() {
            "—".to_owned()
        } else {
            agent.status.clone()
        };
        let mut spans = vec![
            Span::styled(format!(" {mark} "), style),
            Span::styled(format!("{:<16}", agent.name), theme::text()),
            Span::styled(format!("{:<12}", agent.role), theme::dim()),
            Span::styled(format!("{status:<10}"), style),
        ];
        if agent.human {
            spans.push(Span::styled(
                "person · ask, never assign  ",
                theme::accent(),
            ));
        }
        if !agent.note.is_empty() {
            spans.push(Span::styled(agent.note.clone(), theme::dim()));
        }
        lines.push(draw::row(spans, index == cursor));
    }
    frame.render_widget(Paragraph::new(lines).block(draw::panel("Roster")), area);
}

fn workers(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = state::cursor(state);
    let offset = state.agents.len();
    let mut lines = Vec::new();
    if state.workers.is_empty() {
        lines.push(Line::from(Span::styled(
            " No background workers.",
            theme::dim(),
        )));
    }
    for (index, worker) in state.workers.iter().enumerate() {
        let live = worker.process != 0;
        let style = if live { theme::good() } else { theme::dim() };
        // A worker whose supervising session is gone is the one worth noticing:
        // nothing is watching it and nothing will restart it.
        let orphan = live && worker.supervisor == 0;
        let mut spans = vec![
            Span::styled(format!(" {} ", theme::mark(live)), style),
            Span::styled(format!("{:<16}", worker.name), theme::text()),
            Span::styled(format!("{:<12}", worker.role), theme::dim()),
            Span::styled(format!("{:<10}", worker.status), style),
            Span::styled(format!("pid {:<8}", worker.process), theme::dim()),
        ];
        if worker.restarts > 0 {
            spans.push(Span::styled(
                format!("{} restart(s)  ", worker.restarts),
                theme::bad(),
            ));
        }
        if orphan {
            spans.push(Span::styled("unsupervised", theme::bad()));
        }
        lines.push(draw::row(spans, offset + index == cursor));
    }
    frame.render_widget(Paragraph::new(lines).block(draw::panel("Workers")), area);
}

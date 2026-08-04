//! The two settings that change what a session costs.
//!
//! Recall budget and the mesh, both of which decide how much context every
//! connected tool spends before it has done anything. The desktop puts them on
//! one page for that reason and so does this.

use crate::brain::{Optimization, Settings};
use crate::tui::state::State;
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let areas = Layout::vertical([Constraint::Length(10), Constraint::Min(3)]).split(area);
    recall(frame, areas[0], state);
    mesh(frame, areas[1], state);
}

fn recall(frame: &mut Frame, area: Rect, state: &State) {
    let mut lines = vec![
        Line::from(Span::styled(
            " How much a recall may return before it is trimmed.",
            theme::dim(),
        )),
        Line::raw(""),
    ];
    for (key, option) in [
        ('f', Optimization::Full),
        ('b', Optimization::Balanced),
        ('n', Optimization::Lean),
    ] {
        let chosen = state.optimization == option;
        let settings = Settings::from(option);
        let budget = match settings.characterbudget {
            Some(characters) => format!("{characters} characters"),
            None => "no character limit".to_owned(),
        };
        let style = if chosen {
            theme::accent()
        } else {
            theme::dim()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", theme::mark(chosen)), style),
            Span::styled(format!("{key}  "), theme::accent()),
            Span::styled(format!("{:<10}", name(option)), theme::text()),
            Span::styled(
                format!("{:<3} results · {budget}", settings.resultlimit),
                theme::dim(),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " A tool may ask for less than this, never for more.",
        theme::dim(),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel("Recall budget"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn name(optimization: Optimization) -> &'static str {
    match optimization {
        Optimization::Full => "Full",
        Optimization::Balanced => "Balanced",
        Optimization::Lean => "Lean",
    }
}

fn mesh(frame: &mut Frame, area: Rect, state: &State) {
    let on = state.meshenabled;
    let style = if on { theme::good() } else { theme::dim() };
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", theme::mark(on)), style),
            Span::styled("m  ", theme::accent()),
            Span::styled(
                if on {
                    "The mesh is on"
                } else {
                    "The mesh is off"
                },
                theme::text(),
            ),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            " Turning it on adds sixteen coordination tools to every connected",
            theme::dim(),
        )),
        Line::from(Span::styled(
            " session. They cost context whether or not they are used, which is",
            theme::dim(),
        )),
        Line::from(Span::styled(
            " why it stays off until you ask for it.",
            theme::dim(),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            " A session already running keeps the tools it started with.",
            theme::dim(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel("Agent mesh"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

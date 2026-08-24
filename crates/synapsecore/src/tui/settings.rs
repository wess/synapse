//! The settings that change what a session costs.
//!
//! Recall budget, the mesh, and self-improvement: all three decide how much
//! context every connected tool spends before it has done anything. The desktop
//! puts them on one page for that reason and so does this.

use crate::brain::{Optimization, Settings};
use crate::tui::state::State;
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let areas = Layout::vertical([
        Constraint::Length(10),
        Constraint::Min(3),
        Constraint::Length(8),
    ])
    .split(area);
    recall(frame, areas[0], state);
    mesh(frame, areas[1], state);
    learn(frame, areas[2], state);
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

fn learn(frame: &mut Frame, area: Rect, state: &State) {
    let on = state.learnenabled;
    let style = if on { theme::good() } else { theme::dim() };
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", theme::mark(on)), style),
            Span::styled("s  ", theme::accent()),
            Span::styled(
                if on {
                    "Agents may write skills"
                } else {
                    "Agents may not write skills"
                },
                theme::text(),
            ),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            " A skill an agent writes lands in the library and in no tool. It",
            theme::dim(),
        )),
        Line::from(Span::styled(
            " waits on the Skills page until you approve it, so nothing changes",
            theme::dim(),
        )),
        Line::from(Span::styled(
            " how a session behaves without you having read it first.",
            theme::dim(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel("Self-improvement"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

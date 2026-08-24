//! One library, and whether each tool has it.
//!
//! The second list is the important one. A skill Synapse did not write is never
//! overwritten, so showing those separately is not a courtesy — it is the
//! difference between "the library moved on" and "somebody wrote this by hand".
//!
//! A row marked as waiting for review is a skill an agent wrote. It is in the
//! library and in no tool, and it stays that way until somebody presses `a`
//! here — which is the whole of the gate on self-improvement.

use crate::skill::State as SkillState;
use crate::tui::state::{self, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Percentage(35)]).split(area);
    library(frame, areas[0], state);
    unmanaged(frame, areas[1], state);
}

fn library(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = state::cursor(state);
    let mut lines = Vec::new();
    if state.skills.is_empty() {
        lines.push(Line::from(Span::styled(
            " No skills in the library.",
            theme::dim(),
        )));
    }
    for (index, status) in state.skills.iter().enumerate() {
        let (mark, style, word) = match status.proposed {
            true => (theme::mark(false), theme::accent(), "waiting for review"),
            false => describe(status.state),
        };
        lines.push(draw::row(
            vec![
                Span::styled(format!(" {mark} "), style),
                Span::styled(format!("{:<24}", status.skill), theme::text()),
                Span::styled(format!("{:<8}", status.scope), theme::dim()),
                Span::styled(format!("{:<14}", status.tool), theme::dim()),
                Span::styled(word, style),
            ],
            index == cursor,
        ));
    }
    let waiting = state.skills.iter().filter(|status| status.proposed).count();
    let title = match waiting {
        0 => "Library · synapse skill install".to_owned(),
        _ => format!("Library · {waiting} waiting · a approve · d turn down"),
    };
    frame.render_widget(Paragraph::new(lines).block(draw::panel(&title)), area);
}

fn describe(state: SkillState) -> (&'static str, ratatui::style::Style, &'static str) {
    match state {
        SkillState::Installed => (theme::mark(true), theme::good(), "in step"),
        SkillState::Stale => (theme::mark(false), theme::bad(), "library moved on"),
        SkillState::Modified => (theme::mark(false), theme::bad(), "edited after install"),
        SkillState::Missing => (theme::mark(false), theme::dim(), "not installed"),
        SkillState::Foreign => (
            theme::mark(false),
            theme::bad(),
            "written by hand · never overwritten",
        ),
    }
}

fn unmanaged(frame: &mut Frame, area: Rect, state: &State) {
    let mut lines = Vec::new();
    if state.unmanaged.is_empty() {
        lines.push(Line::from(Span::styled(
            " Nothing outside the library.",
            theme::dim(),
        )));
    }
    for name in &state.unmanaged {
        lines.push(Line::from(vec![
            Span::styled("   ", theme::dim()),
            Span::styled(name.clone(), theme::text()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(draw::panel("Skills Synapse did not install")),
        area,
    );
}

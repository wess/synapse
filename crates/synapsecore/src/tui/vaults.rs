//! Vaults, the names inside them, and what this folder resolves to.
//!
//! No value is ever on this screen, and there is no key that would put one
//! there. Secret values live in the Keychain and reach a child process, never a
//! display, a log, or a response — the terminal is not an exception to that.

use crate::tui::state::{self, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(8)]).split(area);
    let columns =
        Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)]).split(rows[0]);
    list(frame, columns[0], state);
    secrets(frame, columns[1], state);
    scope(frame, rows[1], state);
}

fn list(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = state::cursor(state);
    let mut lines = Vec::new();
    if state.vaults.is_empty() {
        lines.push(Line::from(Span::styled(" No vaults yet.", theme::dim())));
    }
    for (index, vault) in state.vaults.iter().enumerate() {
        lines.push(draw::row(
            vec![
                Span::raw(" "),
                Span::styled(vault.name.clone(), theme::text()),
            ],
            index == cursor,
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(draw::panel("Vaults")), area);
}

fn secrets(frame: &mut Frame, area: Rect, state: &State) {
    let mut lines = Vec::new();
    if state.secrets.is_empty() {
        lines.push(Line::from(Span::styled(
            " Nothing in this vault.",
            theme::dim(),
        )));
    }
    for secret in &state.secrets {
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(format!("{:<20}", secret.name), theme::text()),
            Span::styled(format!("{:<24}", secret.env), theme::accent()),
        ];
        if secret.global {
            spans.push(Span::styled("global", theme::dim()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " Values live in the Keychain and are never shown.",
        theme::dim(),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(draw::panel("Names and variables")),
        area,
    );
}

fn scope(frame: &mut Frame, area: Rect, state: &State) {
    let folder = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "this folder".to_owned());
    let mut lines = vec![Line::from(Span::styled(format!(" {folder}"), theme::dim()))];
    match &state.scope {
        None => lines.push(Line::from(Span::styled(
            " No scope resolved.",
            theme::dim(),
        ))),
        Some(resolved) => {
            if resolved.scopes.is_empty() {
                lines.push(Line::from(Span::styled(
                    " No .synapse.yaml applies here.",
                    theme::dim(),
                )));
            }
            for scope in &resolved.scopes {
                // An edited file silently reverts to untrusted and its env is
                // dropped, so `changed` is its own word rather than a shade of
                // "not approved" — the fix is different.
                let (mark, style, word) = if scope.changed {
                    (theme::mark(false), theme::bad(), "changed since approval")
                } else if scope.trusted {
                    (theme::mark(true), theme::good(), "approved")
                } else {
                    (theme::mark(false), theme::dim(), "not approved")
                };
                lines.push(Line::from(vec![
                    Span::styled(format!(" {mark} "), style),
                    Span::styled(format!("{:<44}", scope.path.display()), theme::text()),
                    Span::styled(word, style),
                ]));
            }
            for warning in &resolved.warnings {
                lines.push(Line::from(Span::styled(
                    format!("   {warning}"),
                    theme::bad(),
                )));
            }
            let available = resolved.env.len();
            lines.push(Line::from(Span::styled(
                format!(
                    "   {available} variable(s) would be available to a launched tool",
                    available = available
                ),
                theme::dim(),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(draw::panel("Scope here"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

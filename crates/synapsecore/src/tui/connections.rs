//! Which tools are wired in, and what is left to do.
//!
//! The desktop's first page, and the same reading: one row per tool in one
//! console, plus the three machine-level pieces — the CLI, the shell hook, and
//! `SOUL.md` — that are easy to forget are separate from any single tool.

use crate::cli::InstallStatus;
use crate::shellsetup::IntegrationState;
use crate::tui::state::{Mode, State};
use crate::tui::{draw, theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    if state.mode == Mode::Naming {
        let areas = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(7),
        ])
        .split(area);
        naming(frame, areas[0], state);
        tools(frame, areas[1], state);
        machine(frame, areas[2], state);
        return;
    }
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(7)]).split(area);
    tools(frame, areas[0], state);
    machine(frame, areas[1], state);
}

/// The name of the tool being described, which becomes its file name.
fn naming(frame: &mut Frame, area: Rect, state: &State) {
    let spans = vec![
        Span::styled(" ", theme::text()),
        Span::styled(state.input.clone(), theme::text()),
        // A block rather than a real cursor: the terminal's cursor is parked
        // off-screen while the dashboard owns the display.
        Span::styled("▌", theme::accent()),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(draw::panel(
            "Name for the new tool · lowercase, digits, dashes",
        )),
        area,
    );
}

fn tools(frame: &mut Frame, area: Rect, state: &State) {
    let cursor = crate::tui::state::cursor(state);
    let mut lines = Vec::new();
    if state.connections.is_empty() {
        lines.push(Line::from(Span::styled(
            " No supported tool found on this machine.",
            theme::dim(),
        )));
    }
    for (index, row) in state.connections.iter().enumerate() {
        let found = row.detection.executable.is_some();
        let wired = row.detection.registered && row.detection.configured;
        let (mark, style, word) = if !found {
            (theme::mark(false), theme::dim(), "not installed")
        } else if wired {
            (theme::mark(true), theme::good(), "connected")
        } else if row.detection.registered {
            (
                theme::mark(false),
                theme::bad(),
                "registered, not configured",
            )
        } else {
            (theme::mark(false), theme::dim(), "not connected")
        };
        let version = row
            .detection
            .version
            .clone()
            .unwrap_or_else(|| "—".to_owned());
        lines.push(draw::row(
            vec![
                Span::styled(format!(" {mark} "), style),
                Span::styled(format!("{:<14}", row.agent.name), theme::text()),
                Span::styled(format!("{version:<12}"), theme::dim()),
                Span::styled(word, style),
            ],
            index == cursor,
        ));
    }
    // The row past the end. There will be more coding tools than Synapse ships
    // descriptors for, and this is where a person says so.
    lines.push(draw::row(
        vec![
            Span::styled("   ", theme::dim()),
            Span::styled("+ Add a connection…", theme::dim()),
        ],
        cursor == state.connections.len(),
    ));
    frame.render_widget(
        Paragraph::new(lines).block(draw::panel("Connected tools")),
        area,
    );
}

fn machine(frame: &mut Frame, area: Rect, state: &State) {
    let (climark, clistyle, cliword) = match &state.cli {
        InstallStatus::Installed(path) => (
            theme::mark(true),
            theme::good(),
            format!("installed at {}", path.display()),
        ),
        InstallStatus::Conflict(path) => (
            theme::mark(false),
            theme::bad(),
            format!("something else is at {}", path.display()),
        ),
        InstallStatus::Missing => (
            theme::mark(false),
            theme::dim(),
            "not installed · synapse install".to_owned(),
        ),
    };

    let (shellmark, shellstyle, shellword) = match &state.shell {
        Some(integration) => match integration.state {
            IntegrationState::Installed => (
                theme::mark(true),
                theme::good(),
                format!("{} · {}", integration.shell, integration.path.display()),
            ),
            IntegrationState::Modified => (
                theme::mark(false),
                theme::bad(),
                format!("edited by hand in {}", integration.path.display()),
            ),
            IntegrationState::Missing => (
                theme::mark(false),
                theme::dim(),
                format!("not installed in {}", integration.path.display()),
            ),
        },
        None => (
            theme::mark(false),
            theme::dim(),
            "no supported shell found".to_owned(),
        ),
    };

    let (soulmark, soulstyle, soulword) = match &state.guidance {
        None => (theme::mark(false), theme::dim(), "not read yet".to_owned()),
        Some(guidance) if !guidance.exists => (
            theme::mark(false),
            theme::dim(),
            "not written yet".to_owned(),
        ),
        // A block from an older release is a different problem from never
        // having been set up, and it wants a different fix: sync, not connect.
        Some(guidance) if guidance.stale > 0 => (
            theme::mark(false),
            theme::bad(),
            format!("{} tool(s) carry an older block", guidance.stale),
        ),
        Some(guidance) => (
            theme::mark(true),
            theme::good(),
            format!("pointed at by {} of {}", guidance.synced, guidance.total),
        ),
    };

    let lines = vec![
        line("CLI", climark, clistyle, &cliword),
        line("Shell hook", shellmark, shellstyle, &shellword),
        line("SOUL.md", soulmark, soulstyle, &soulword),
        Line::raw(""),
        Line::from(Span::styled(
            format!("  {}", state.database.display()),
            theme::dim(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(draw::panel("This machine")),
        area,
    );
}

fn line<'a>(label: &'a str, mark: &'a str, style: ratatui::style::Style, detail: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {mark} "), style),
        Span::styled(format!("{label:<12}"), theme::text()),
        Span::styled(detail.to_owned(), theme::dim()),
    ])
}

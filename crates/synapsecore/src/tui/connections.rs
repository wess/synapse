//! Which tools are wired in, and what is left to do.
//!
//! The desktop's first page, and the same reading: one row per tool, plus the
//! three machine-level pieces — the CLI, the shell hook, and `SOUL.md` — that
//! are easy to forget are separate from any single tool.
//!
//! Two lists rather than one. What is connected is the shorter list and the one
//! somebody acts on, and a flat list buried it among tools they do not have. The
//! split is in the *order* — [`crate::agent::connections`] sorts connected first
//! — so the cursor stays one index into one vector and the window can draw the
//! same partition without deciding it again.

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
        lists(frame, areas[1], state);
        machine(frame, areas[2], state);
        return;
    }
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(7)]).split(area);
    lists(frame, areas[0], state);
    machine(frame, areas[1], state);
}

/// The connected half above the rest.
///
/// Each panel is given room for its own rows and no more, so neither is a
/// mostly-empty box — but the connected one is capped at half the space, or a
/// machine with six connected tools would leave nothing for the list of what it
/// could still add. Below the height where both fit, the supported list is what
/// gives way: it is a catalogue, and the connected list is the live state of the
/// machine.
fn lists(frame: &mut Frame, area: Rect, state: &State) {
    let connected = crate::tui::state::connectedcount(state);
    // Two lines of chrome per panel, and the lower one always carries the row
    // that adds a tool even when every supported tool is already connected.
    let wanted = (connected as u16).saturating_add(2);
    let ceiling = area.height / 2;
    let top = wanted.min(ceiling.max(3));
    let areas = Layout::vertical([Constraint::Length(top), Constraint::Min(3)]).split(area);
    tools(frame, areas[0], state, 0..connected, true);
    tools(
        frame,
        areas[1],
        state,
        connected..state.connections.len(),
        false,
    );
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

/// One panel over a slice of the single row vector. `range` is what to draw and
/// `wired` says which half it is, which is the only thing that differs between
/// them: the indices stay absolute so the cursor needs no translating.
fn tools(frame: &mut Frame, area: Rect, state: &State, range: std::ops::Range<usize>, wired: bool) {
    let cursor = crate::tui::state::cursor(state);
    let mut lines = Vec::new();
    if range.is_empty() {
        lines.push(Line::from(Span::styled(
            if wired {
                " Nothing is connected yet. Move to a tool below and press c."
            } else {
                " Every supported tool on this machine is connected."
            },
            theme::dim(),
        )));
    }
    for index in range.clone() {
        let Some(row) = state.connections.get(index) else {
            continue;
        };
        let (mark, style, word) = if !row.installed() {
            (theme::mark(false), theme::dim(), "not installed")
        } else if row.outdated {
            // Connected, and would be connected differently by this release.
            // Not a fault and not an error — but the one row on the page with
            // something to do about it, so it does not read as merely fine.
            (theme::mark(true), theme::bad(), "update available · u")
        } else if row.connected() {
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
        // Held to the column rather than allowed to push the state along the
        // row: `claude --version` answers "2.1.259 (Claude Code)", which ran
        // straight into the word beside it.
        let version = fit(
            row.detection.version.as_deref().unwrap_or("—"),
            VERSIONWIDTH,
        );
        lines.push(draw::row(
            vec![
                Span::styled(format!(" {mark} "), style),
                Span::styled(format!("{:<14}", row.agent.name), theme::text()),
                // One wider than the cut, so a version that fills the column
                // still has a space between it and the state.
                Span::styled(
                    format!("{version:<width$}", width = VERSIONWIDTH + 1),
                    theme::dim(),
                ),
                Span::styled(word, style),
            ],
            index == cursor,
        ));
    }
    if !wired {
        // The row past the end. There will be more coding tools than Synapse
        // ships descriptors for, and this is where a person says so.
        lines.push(draw::row(
            vec![
                Span::styled("   ", theme::dim()),
                Span::styled("+ Add a connection…", theme::dim()),
            ],
            cursor == state.connections.len(),
        ));
    }
    let title = if wired {
        format!("Connected · {} · u update  R reset  d remove", range.len())
    } else {
        format!("Supported · {} · c connect  e describe", range.len())
    };
    frame.render_widget(Paragraph::new(lines).block(draw::panel(&title)), area);
}

/// How much room a version gets before it is cut. Wide enough for a semantic
/// version with a suffix, narrow enough that the state beside it starts in the
/// same place on every row.
const VERSIONWIDTH: usize = 13;

/// `text` at no more than `width` columns, with the cut marked. Counts
/// characters rather than bytes, because a version string is not guaranteed to
/// be ASCII and slicing one in the middle of a character would panic.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
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

//! Everything on screen, as data.
//!
//! The same six pages the desktop has, in the same order, so a person who knows
//! one knows the other. Plain fields and free functions rather than a widget
//! tree: the terminal redraws the whole frame every tick anyway, so there is
//! nothing here to keep in step but the numbers themselves.

use crate::agent::{Agent, Detection, GuidanceState};
use crate::brain::{Memory, Optimization, Stats};
use crate::cli::InstallStatus;
use crate::relay::{AgentView, WorkerView};
use crate::shellsetup::Integration;
use crate::skill::Status as SkillStatus;
use crate::vault::{Resolved, Secret, Vault};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Connections,
    Memories,
    Mesh,
    Skills,
    Vaults,
    Settings,
}

/// The order they appear in, which is also what the number keys reach.
///
/// Grouped rather than historical: the sidebar draws a heading whenever the
/// group changes, so a page filed out of order would print its group twice. It
/// matches the desktop's column for the same reason the pages themselves do.
pub const PAGES: [Page; 6] = [
    Page::Connections,
    Page::Memories,
    Page::Skills,
    Page::Mesh,
    Page::Vaults,
    Page::Settings,
];

pub fn title(page: Page) -> &'static str {
    match page {
        Page::Connections => "Connections",
        Page::Memories => "Memories",
        Page::Mesh => "Mesh",
        Page::Skills => "Skills",
        Page::Vaults => "Vaults",
        Page::Settings => "Settings",
    }
}

/// Which group of the sidebar a page belongs to. Matches the desktop's, so a
/// person moving between the two finds the same things under the same heading.
pub fn section(page: Page) -> &'static str {
    match page {
        Page::Connections | Page::Memories | Page::Skills => "Workspace",
        Page::Mesh => "Agents",
        Page::Vaults | Page::Settings => "System",
    }
}

#[derive(Clone)]
pub enum Notice {
    Ready,
    Success(String),
    Error(String),
}

impl Notice {
    pub fn message(&self) -> &str {
        match self {
            // The desktop's words, because the promise is the same one.
            Notice::Ready => {
                "Memory stays on this machine and is shared only with tools you connect."
            }
            Notice::Success(message) | Notice::Error(message) => message,
        }
    }
}

/// What the user is typing into, if anything. A page that owns no input is
/// always in `Browse`, and every key is a command rather than a character.
#[derive(Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Search,
    /// Typing the name of a tool to describe. It becomes the descriptor's file
    /// name, so it is taken here and validated before an editor opens.
    Naming,
    Help,
    /// A destructive action, waiting to be confirmed or abandoned. Nothing
    /// irreversible happens on a single keypress.
    Confirm(Pending),
}

#[derive(Clone, PartialEq, Eq)]
pub enum Pending {
    DeleteMemory(i64),
    /// Taking a tool back out. Recoverable, unlike a memory, but it edits
    /// somebody else's configuration and so is still asked about first.
    Disconnect(String),
    /// Turning down a proposed skill, which deletes it. The name and the
    /// project it belongs to, which together are the skill.
    RejectSkill(String, String),
}

pub struct Connection {
    pub agent: Agent,
    pub detection: Detection,
}

pub struct State {
    pub page: Page,
    pub mode: Mode,
    pub notice: Notice,
    pub quit: bool,

    pub database: PathBuf,
    pub stats: Stats,
    pub optimization: Optimization,
    pub meshenabled: bool,
    pub learnenabled: bool,

    pub connections: Vec<Connection>,
    pub cli: InstallStatus,
    pub shell: Option<Integration>,
    pub guidance: Option<GuidanceState>,

    pub memories: Vec<Memory>,
    pub query: String,
    /// What is being typed outside the memory search: the name of a new tool.
    pub input: String,

    pub agents: Vec<AgentView>,
    pub workers: Vec<WorkerView>,
    pub mesherror: Option<String>,

    pub skills: Vec<SkillStatus>,
    pub unmanaged: Vec<String>,

    pub vaults: Vec<Vault>,
    pub secrets: Vec<Secret>,
    pub scope: Option<Resolved>,

    /// One cursor per page, so moving away and back does not lose your place.
    pub cursor: [usize; PAGES.len()],
}

pub fn slot(page: Page) -> usize {
    PAGES.iter().position(|item| *item == page).unwrap_or(0)
}

/// How many rows the current page can move through. Pages that are pure report
/// have none, and the cursor keys are simply inert there.
pub fn rows(state: &State) -> usize {
    match state.page {
        // One past the tools, for the row that adds another.
        Page::Connections => state.connections.len() + 1,
        Page::Memories => state.memories.len(),
        Page::Mesh => state.agents.len() + state.workers.len(),
        Page::Skills => state.skills.len(),
        Page::Vaults => state.vaults.len(),
        Page::Settings => 0,
    }
}

pub fn cursor(state: &State) -> usize {
    let count = rows(state);
    if count == 0 {
        return 0;
    }
    state.cursor[slot(state.page)].min(count - 1)
}

pub fn setcursor(state: &mut State, value: usize) {
    let index = slot(state.page);
    state.cursor[index] = value;
}

pub fn move_(state: &mut State, delta: isize) {
    let count = rows(state);
    if count == 0 {
        return;
    }
    let current = cursor(state) as isize;
    setcursor(
        state,
        (current + delta).clamp(0, count as isize - 1) as usize,
    );
}

/// The memory the cursor is on, when the memories page is showing one.
pub fn selectedmemory(state: &State) -> Option<&Memory> {
    if state.page != Page::Memories {
        return None;
    }
    state.memories.get(cursor(state))
}

/// The skill row the cursor is on, when the skills page is showing one.
pub fn selectedskill(state: &State) -> Option<&SkillStatus> {
    if state.page != Page::Skills {
        return None;
    }
    state.skills.get(cursor(state))
}

/// The tool the cursor is on. `None` on the last row, which is the one that
/// adds a tool rather than naming one.
pub fn selectedtool(state: &State) -> Option<&Connection> {
    if state.page != Page::Connections {
        return None;
    }
    state.connections.get(cursor(state))
}

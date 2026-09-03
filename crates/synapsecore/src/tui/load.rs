//! Filling the screen from the store.
//!
//! Every read here is the one the desktop makes for the same panel, so the two
//! surfaces cannot drift into disagreeing about the same machine. Reads are
//! `glance` wherever the answer is a report: this loop runs on a keypress, and
//! a whole-page integrity scan on every refresh would cost more than everything
//! else it does put together.
//!
//! A failure loads as an empty section with a notice, never as an exit. A
//! machine with no mesh, no vault, and no connected tool is a normal machine,
//! and the dashboard for it should still draw.

use crate::agent;
use crate::brain::Brain;
use crate::relay::Mesh;
use crate::tui::state::{Mode, Notice, PAGES, State};
use crate::vault::VaultStore;
use crate::{cli, files, shellsetup, skill, vault};
use anyhow::Result;

/// The number of memories the list holds. Enough to scroll through a real
/// store, bounded because every one of them is a row this process keeps.
const MEMORIES: u32 = 500;

pub async fn initial() -> Result<State> {
    let database = files::database()?;
    let mut state = State {
        page: PAGES[0],
        mode: Mode::Browse,
        notice: Notice::Ready,
        quit: false,
        database,
        stats: Default::default(),
        optimization: Default::default(),
        meshenabled: false,
        learnenabled: false,
        connections: Vec::new(),
        cli: cli::InstallStatus::Missing,
        shell: None,
        guidance: None,
        memories: Vec::new(),
        query: String::new(),
        input: String::new(),
        agents: Vec::new(),
        workers: Vec::new(),
        mesherror: None,
        skills: Vec::new(),
        unmanaged: Vec::new(),
        vaults: Vec::new(),
        secrets: Vec::new(),
        backend: vault::Backend::Encrypted,
        scope: None,
        cursor: [0; PAGES.len()],
    };
    refresh(&mut state).await;
    Ok(state)
}

/// Reload everything. Sections are loaded independently and a failed one leaves
/// its own last-known contents alone rather than emptying the screen.
pub async fn refresh(state: &mut State) {
    memories(state).await;
    connections(state).await;
    mesh(state).await;
    skills(state).await;
    vaults(state).await;
}

pub async fn memories(state: &mut State) {
    let brain = match Brain::glance(&state.database).await {
        Ok(brain) => brain,
        Err(error) => {
            state.notice = Notice::Error(format!("could not open memory: {error}"));
            return;
        }
    };
    match brain.search(&state.query, MEMORIES).await {
        Ok(found) => state.memories = found,
        Err(error) => state.notice = Notice::Error(format!("could not search memory: {error}")),
    }
    if let Ok(stats) = brain.stats().await {
        state.stats = stats;
    }
    if let Ok(settings) = brain.settings().await {
        state.optimization = settings.optimization;
    }
    if let Ok(enabled) = brain.mesh().await {
        state.meshenabled = enabled;
    }
    if let Ok(enabled) = brain.learn().await {
        state.learnenabled = enabled;
    }
}

async fn connections(state: &mut State) {
    let Ok(home) = files::home() else {
        return;
    };
    let server = cli::destination().ok();
    state.connections = agent::connections(&home, server.as_deref(), &state.database).await;
    state.cli = cli::status().unwrap_or(cli::InstallStatus::Missing);
    state.shell = cli::destination()
        .ok()
        .and_then(|command| shellsetup::status(&command).ok());
    if let Ok(soul) = files::soul() {
        state.guidance = Some(agent::guidancestate(&home, &soul));
    }
}

async fn mesh(state: &mut State) {
    // The mesh is off by default and its tables are only interesting when it is
    // on. Reading them anyway would report an empty roster as though somebody
    // had turned it on and nobody had joined.
    if !state.meshenabled {
        state.agents.clear();
        state.workers.clear();
        state.mesherror = None;
        return;
    }
    match Mesh::glance(&state.database).await {
        Ok(mesh) => {
            state.agents = mesh.agents().await.unwrap_or_default();
            state.workers = mesh.workers().await.unwrap_or_default();
            state.mesherror = None;
        }
        Err(error) => state.mesherror = Some(error.to_string()),
    }
}

async fn skills(state: &mut State) {
    let Ok(home) = files::home() else {
        return;
    };
    match skill::survey(&home).await {
        Ok((statuses, unknown)) => {
            state.skills = statuses;
            state.unmanaged = unknown;
        }
        Err(error) => state.notice = Notice::Error(format!("could not read skills: {error}")),
    }
}

async fn vaults(state: &mut State) {
    let store = match VaultStore::glance(&state.database).await {
        Ok(store) => store,
        Err(error) => {
            state.notice = Notice::Error(format!("could not open the vault: {error}"));
            return;
        }
    };
    state.backend = vault::backend().await.unwrap_or(vault::Backend::Encrypted);
    state.vaults = store.vaults().await.unwrap_or_default();
    let selected = state
        .cursor
        .get(crate::tui::state::slot(crate::tui::state::Page::Vaults))
        .copied()
        .unwrap_or(0);
    state.secrets = match state
        .vaults
        .get(selected.min(state.vaults.len().saturating_sub(1)))
    {
        Some(vault) => store.secrets(vault.id).await.unwrap_or_default(),
        None => Vec::new(),
    };
    // Scope resolution answers for the directory the dashboard was opened in,
    // which is the one whose `.synapse.yaml` the person is asking about.
    if let Ok(folder) = std::env::current_dir() {
        state.scope = vault::resolve(&store, &folder).await.ok();
    }
}

/// Reload only the secrets for the vault under the cursor. Moving down a list of
/// vaults should not re-scan every tool on the machine.
pub async fn secrets(state: &mut State) {
    let Some(vault) = state.vaults.get(crate::tui::state::cursor(state)) else {
        state.secrets = Vec::new();
        return;
    };
    if let Ok(store) = VaultStore::glance(&state.database).await {
        state.secrets = store.secrets(vault.id).await.unwrap_or_default();
    }
}

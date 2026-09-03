//! Keys in, intent out.
//!
//! This module decides nothing and touches nothing: it turns a keypress into an
//! [`Action`] and lets the loop do the work. Anything that writes goes through
//! `Confirm` first, so no single key can delete a memory or wipe a store.

use crate::brain::Optimization;
use crate::tui::state::{self, Mode, Notice, PAGES, Page, Pending, State};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum Action {
    None,
    Refresh,
    /// The vault list moved; only that page's secrets need re-reading.
    Secrets,
    DeleteMemory(i64),
    SetOptimization(Optimization),
    ToggleMesh,
    ToggleLearn,
    /// Install a proposed skill and stop calling it proposed. The skill's name
    /// and the project it belongs to, empty for a global one.
    ApproveSkill(String, String),
    /// Turn one down, which removes it from the library.
    RejectSkill(String, String),
    /// Wire the tool under the cursor into memory and the vault.
    Connect(String),
    /// Apply this release's descriptor to a tool that is already connected, so
    /// a change that shipped after it was set up actually reaches it.
    Reapply(String),
    /// Take the connection out and make it again.
    Reset(String),
    /// Undo that.
    Disconnect(String),
    /// Open a tool's descriptor in `$EDITOR`, seeded from the existing one or
    /// from the template when there is none yet.
    Describe(String),
}

pub fn handle(state: &mut State, key: KeyEvent) -> Action {
    match state.mode.clone() {
        Mode::Search => search(state, key),
        Mode::Naming => naming(state, key),
        Mode::Help => {
            state.mode = Mode::Browse;
            Action::None
        }
        Mode::Confirm(pending) => confirm(state, key, pending),
        Mode::Browse => browse(state, key),
    }
}

fn search(state: &mut State, key: KeyEvent) -> Action {
    match key.code {
        // Both leave the search box; neither throws the query away, because
        // retyping what you just typed is the most annoying possible outcome.
        KeyCode::Esc | KeyCode::Enter => {
            state.mode = Mode::Browse;
            Action::Refresh
        }
        KeyCode::Backspace => {
            state.query.pop();
            Action::Refresh
        }
        KeyCode::Char(character) => {
            state.query.push(character);
            Action::Refresh
        }
        _ => Action::None,
    }
}

/// Typing the name of a tool to describe. It becomes a file name, so it is
/// checked here rather than after an editor has been opened on a draft that can
/// never be saved.
fn naming(state: &mut State, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.mode = Mode::Browse;
            state.input.clear();
            state.notice = Notice::Ready;
            Action::None
        }
        KeyCode::Enter => {
            let name = state.input.trim().to_owned();
            match crate::relay::validlayername(&name) {
                Ok(()) => {
                    state.mode = Mode::Browse;
                    state.input.clear();
                    Action::Describe(name)
                }
                Err(error) => {
                    state.notice = Notice::Error(error.to_string());
                    Action::None
                }
            }
        }
        KeyCode::Backspace => {
            state.input.pop();
            Action::None
        }
        KeyCode::Char(character) => {
            state.input.push(character);
            Action::None
        }
        _ => Action::None,
    }
}

fn confirm(state: &mut State, key: KeyEvent, pending: Pending) -> Action {
    // Only `y` confirms. Every other key, including Enter, abandons — Enter is
    // what a person presses to dismiss something they did not read.
    let action = match (key.code, pending) {
        (KeyCode::Char('y'), Pending::DeleteMemory(id)) => Action::DeleteMemory(id),
        (KeyCode::Char('y'), Pending::Disconnect(slug)) => Action::Disconnect(slug),
        (KeyCode::Char('y'), Pending::RejectSkill(name, project)) => {
            Action::RejectSkill(name, project)
        }
        (KeyCode::Char('y'), Pending::Reset(slug)) => Action::Reset(slug),
        _ => {
            state.notice = Notice::Ready;
            Action::None
        }
    };
    state.mode = Mode::Browse;
    action
}

fn browse(state: &mut State, key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        state.quit = true;
        return Action::None;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.quit = true;
            Action::None
        }
        KeyCode::Char('?') => {
            state.mode = Mode::Help;
            Action::None
        }
        KeyCode::Char('r') => Action::Refresh,

        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => turn(state, 1),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => turn(state, -1),
        KeyCode::Char(digit @ '1'..='6') => {
            let index = digit as usize - '1' as usize;
            state.page = PAGES[index];
            page(state)
        }

        KeyCode::Down | KeyCode::Char('j') => {
            state::move_(state, 1);
            moved(state)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state::move_(state, -1);
            moved(state)
        }
        KeyCode::PageDown => {
            state::move_(state, 10);
            moved(state)
        }
        KeyCode::PageUp => {
            state::move_(state, -10);
            moved(state)
        }
        KeyCode::Home | KeyCode::Char('g') => {
            state::setcursor(state, 0);
            moved(state)
        }
        KeyCode::End | KeyCode::Char('G') => {
            state::setcursor(state, state::rows(state).saturating_sub(1));
            moved(state)
        }

        KeyCode::Char('/') if state.page == Page::Memories => {
            state.mode = Mode::Search;
            Action::None
        }
        KeyCode::Char('d') if state.page == Page::Memories => match state::selectedmemory(state) {
            Some(memory) => {
                let id = memory.id;
                state.notice = Notice::Error(format!("delete memory #{id}? y to confirm"));
                state.mode = Mode::Confirm(Pending::DeleteMemory(id));
                Action::None
            }
            None => Action::None,
        },

        // The row past the end of the tool list asks for a name and then opens
        // a descriptor; every other row connects the tool it names.
        KeyCode::Enter | KeyCode::Char('c') if state.page == Page::Connections => {
            match state::selectedtool(state) {
                Some(row) => Action::Connect(row.agent.slug.clone()),
                None => {
                    state.mode = Mode::Naming;
                    state.input.clear();
                    Action::None
                }
            }
        }
        KeyCode::Char('e') if state.page == Page::Connections => match state::selectedtool(state) {
            Some(row) => Action::Describe(row.agent.slug.clone()),
            None => {
                state.mode = Mode::Naming;
                state.input.clear();
                Action::None
            }
        },
        KeyCode::Char('d') if state.page == Page::Connections => match state::selectedtool(state) {
            Some(row) => {
                let slug = row.agent.slug.clone();
                state.notice = Notice::Error(format!("disconnect {slug}? y to confirm"));
                state.mode = Mode::Confirm(Pending::Disconnect(slug));
                Action::None
            }
            None => Action::None,
        },

        // Re-applying writes the registration again and removes nothing, so it
        // happens on one key. Resetting disconnects first, which takes the
        // tool's installed skills with it, so it does not.
        //
        // `u` rather than `r`, which is the global reload and reaches this page
        // first — and `update` is the word the row itself uses when a release
        // has moved its descriptor.
        KeyCode::Char('u') if state.page == Page::Connections => match state::selectedtool(state) {
            Some(row) if row.connected() => Action::Reapply(row.agent.slug.clone()),
            Some(_) => {
                state.notice = Notice::Error("that tool is not connected yet".to_owned());
                Action::None
            }
            None => Action::None,
        },
        KeyCode::Char('R') if state.page == Page::Connections => match state::selectedtool(state) {
            Some(row) if row.connected() => {
                let slug = row.agent.slug.clone();
                state.notice =
                    Notice::Error(format!("reset {slug}? it disconnects first · y to confirm"));
                state.mode = Mode::Confirm(Pending::Reset(slug));
                Action::None
            }
            Some(_) => {
                state.notice = Notice::Error("that tool is not connected yet".to_owned());
                Action::None
            }
            None => Action::None,
        },

        // Approving is not destructive, so it happens on one key. Turning a
        // skill down deletes it, so it does not.
        KeyCode::Enter | KeyCode::Char('a') if state.page == Page::Skills => {
            match state::selectedskill(state)
                .map(|row| (row.proposed, row.skill.clone(), row.project.clone()))
            {
                Some((true, name, project)) => Action::ApproveSkill(name, project),
                Some((false, ..)) => {
                    state.notice = Notice::Error("that skill is not waiting for review".to_owned());
                    Action::None
                }
                None => Action::None,
            }
        }
        KeyCode::Char('d') if state.page == Page::Skills => {
            match state::selectedskill(state)
                .map(|row| (row.proposed, row.skill.clone(), row.project.clone()))
            {
                Some((true, name, project)) => {
                    state.notice = Notice::Error(format!("turn down `{name}`? y to confirm"));
                    state.mode = Mode::Confirm(Pending::RejectSkill(name, project));
                    Action::None
                }
                Some((false, ..)) => {
                    state.notice = Notice::Error(
                        "only a proposed skill is turned down here; use `synapse skill delete`"
                            .to_owned(),
                    );
                    Action::None
                }
                None => Action::None,
            }
        }

        KeyCode::Char('f') if state.page == Page::Settings => {
            Action::SetOptimization(Optimization::Full)
        }
        KeyCode::Char('b') if state.page == Page::Settings => {
            Action::SetOptimization(Optimization::Balanced)
        }
        KeyCode::Char('n') if state.page == Page::Settings => {
            Action::SetOptimization(Optimization::Lean)
        }
        KeyCode::Char('m') if state.page == Page::Settings => Action::ToggleMesh,
        KeyCode::Char('s') if state.page == Page::Settings => Action::ToggleLearn,

        _ => Action::None,
    }
}

fn turn(state: &mut State, delta: isize) -> Action {
    let at = state::slot(state.page) as isize;
    let count = PAGES.len() as isize;
    state.page = PAGES[((at + delta).rem_euclid(count)) as usize];
    page(state)
}

/// Arriving at a page. Only the vault page has to fetch anything, because its
/// second column belongs to whichever row the cursor happens to be on.
fn page(state: &State) -> Action {
    match state.page {
        Page::Vaults => Action::Secrets,
        _ => Action::None,
    }
}

fn moved(state: &State) -> Action {
    page(state)
}

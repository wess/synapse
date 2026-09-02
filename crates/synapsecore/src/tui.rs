//! The dashboard, in a terminal.
//!
//! The same six pages the desktop draws, reading the same store through the
//! same functions. It exists because a machine with no display still has
//! memory, connected tools, and a mesh worth looking at, and `synapse status`
//! answers three lines of that.
//!
//! Nothing here holds a handle open. Every refresh opens the store, reads, and
//! drops it, which costs a few milliseconds on a keypress and means the
//! dashboard never becomes the reason `synapse data restore` reports that
//! something is using the database.

mod connections;
mod draw;
mod keys;
mod load;
mod memories;
mod mesh;
mod settings;
mod skills;
mod state;
mod theme;
mod vaults;

use anyhow::{Context, Result};
use keys::Action;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal;
use state::Notice;
use std::time::Duration;

/// How long a frame waits for a key before drawing again. Long enough that an
/// idle dashboard is not a busy loop, short enough that a keypress feels
/// immediate.
const TICK: Duration = Duration::from_millis(250);

/// Whether this process is attached to a terminal that can be drawn on.
///
/// Checked before anything is set up, because entering raw mode on a pipe
/// corrupts the output of whatever is reading it, and a person who ran
/// `synapse | less` asked for text.
///
/// A size of zero counts as no terminal. Some pseudo-terminals — the one
/// `script` allocates, and a few CI runners — report success and no room, and
/// drawing into that is a loop that clears the screen forever and shows
/// nothing. Falling through to the text form is the useful answer.
pub fn available() -> bool {
    terminal::size().is_ok_and(|(columns, rows)| columns > 0 && rows > 0)
}

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("could not start the async runtime")?;
    let mut state = runtime.block_on(load::initial())?;

    let mut terminal = ratatui::init();
    let outcome = loop {
        if let Err(error) = terminal.draw(|frame| draw::frame(frame, &state)) {
            break Err(error).context("could not draw the dashboard");
        }
        match pump(&runtime, &mut state, &mut terminal) {
            Ok(()) if state.quit => break Ok(()),
            Ok(()) => {}
            Err(error) => break Err(error),
        }
    };
    // Restore before returning either way: a terminal left in raw mode is worse
    // than whatever went wrong.
    ratatui::restore();
    outcome
}

/// One pass of the event loop: wait for input, turn it into an action, run it.
fn pump(
    runtime: &tokio::runtime::Runtime,
    state: &mut state::State,
    terminal: &mut ratatui::DefaultTerminal,
) -> Result<()> {
    if !event::poll(TICK).context("could not read the terminal")? {
        return Ok(());
    }
    let Event::Key(key) = event::read().context("could not read the terminal")? else {
        return Ok(());
    };
    // Windows reports press and release; acting on both double-handles a key.
    if key.kind != KeyEventKind::Press {
        return Ok(());
    }
    match keys::handle(state, key) {
        Action::None => Ok(()),
        Action::Refresh => {
            runtime.block_on(load::refresh(state));
            Ok(())
        }
        Action::Secrets => {
            runtime.block_on(load::secrets(state));
            Ok(())
        }
        Action::DeleteMemory(id) => {
            runtime.block_on(async {
                match deletememory(state, id).await {
                    Ok(()) => state.notice = Notice::Success(format!("deleted memory #{id}")),
                    Err(error) => state.notice = Notice::Error(error.to_string()),
                }
                load::memories(state).await;
            });
            Ok(())
        }
        Action::SetOptimization(optimization) => {
            runtime.block_on(async {
                let brain = crate::brain::Brain::open(&state.database).await;
                match brain {
                    Ok(brain) => match brain.setoptimization(optimization).await {
                        Ok(()) => {
                            state.optimization = optimization;
                            state.notice = Notice::Success("recall budget saved".to_owned());
                        }
                        Err(error) => state.notice = Notice::Error(error.to_string()),
                    },
                    Err(error) => state.notice = Notice::Error(error.to_string()),
                }
            });
            Ok(())
        }
        Action::ToggleMesh => {
            runtime.block_on(async {
                let wanted = !state.meshenabled;
                match crate::brain::Brain::open(&state.database).await {
                    Ok(brain) => match brain.setmesh(wanted).await {
                        Ok(()) => {
                            state.meshenabled = wanted;
                            state.notice = Notice::Success(
                                if wanted {
                                    "the mesh is on · new sessions get the tools"
                                } else {
                                    "the mesh is off · running sessions keep theirs"
                                }
                                .to_owned(),
                            );
                        }
                        Err(error) => state.notice = Notice::Error(error.to_string()),
                    },
                    Err(error) => state.notice = Notice::Error(error.to_string()),
                }
                load::refresh(state).await;
            });
            Ok(())
        }
        Action::ToggleLearn => {
            runtime.block_on(async {
                let wanted = !state.learnenabled;
                match crate::brain::Brain::open(&state.database).await {
                    Ok(brain) => match brain.setlearn(wanted).await {
                        Ok(()) => {
                            state.learnenabled = wanted;
                            state.notice = Notice::Success(
                                if wanted {
                                    "agents can write skills · they wait here for you"
                                } else {
                                    "agents can no longer write skills"
                                }
                                .to_owned(),
                            );
                        }
                        Err(error) => state.notice = Notice::Error(error.to_string()),
                    },
                    Err(error) => state.notice = Notice::Error(error.to_string()),
                }
                load::refresh(state).await;
            });
            Ok(())
        }
        Action::ApproveSkill(name, project) => {
            state.notice = match runtime.block_on(approveskill(&name, &project)) {
                Ok(tools) if tools.is_empty() => {
                    Notice::Error(format!("no connected tool can hold `{name}`"))
                }
                Ok(tools) => Notice::Success(format!("{name} → {}", tools.join(", "))),
                Err(error) => Notice::Error(format!("{error:#}")),
            };
            runtime.block_on(load::refresh(state));
            Ok(())
        }
        Action::RejectSkill(name, project) => {
            state.notice = match runtime.block_on(rejectskill(&name, &project)) {
                Ok(()) => Notice::Success(format!("turned down `{name}`")),
                Err(error) => Notice::Error(format!("{error:#}")),
            };
            runtime.block_on(load::refresh(state));
            Ok(())
        }
        Action::Connect(slug) => {
            state.notice = match connect(&slug) {
                Ok(name) => Notice::Success(format!("{name} is connected to Synapse")),
                Err(error) => Notice::Error(format!("{error:#}")),
            };
            runtime.block_on(load::refresh(state));
            Ok(())
        }
        Action::Disconnect(slug) => {
            state.notice = match runtime.block_on(disconnect(&slug)) {
                Ok(message) => Notice::Success(message),
                Err(error) => Notice::Error(format!("{error:#}")),
            };
            runtime.block_on(load::refresh(state));
            Ok(())
        }
        // The editor owns the terminal for as long as it runs, so the dashboard
        // gives it back and takes it again afterwards. Restoring first is not
        // optional: an editor started inside raw mode draws over the dashboard
        // and leaves the shell unusable when it exits.
        Action::Describe(slug) => {
            ratatui::restore();
            let outcome = crate::cli::describetool(&slug);
            *terminal = ratatui::init();
            terminal.clear().ok();
            state.notice = match outcome {
                Ok(path) => Notice::Success(format!("saved {}", path.display())),
                Err(error) => Notice::Error(format!("{error:#}")),
            };
            runtime.block_on(load::refresh(state));
            Ok(())
        }
    }
}

/// Wire one tool in, by the slug the dashboard row carries.
fn connect(slug: &str) -> Result<String> {
    let home = crate::files::home()?;
    let server = crate::cli::destination()?;
    let soul = crate::files::soul()?;
    let agent = crate::agent::agents(&home)
        .into_iter()
        .find(|agent| agent.slug == slug)
        .with_context(|| format!("no tool named `{slug}`"))?;
    let detection = crate::agent::detect(&agent, Some(&server));
    crate::agent::setup(&agent, &detection, &server, &soul)?;
    Ok(agent.name)
}

async fn disconnect(slug: &str) -> Result<String> {
    let home = crate::files::home()?;
    let server = crate::cli::destination()?;
    let agent = crate::agent::agents(&home)
        .into_iter()
        .find(|agent| agent.slug == slug)
        .with_context(|| format!("no tool named `{slug}`"))?;
    let removed = crate::agent::disconnect(&agent, &server).await;
    anyhow::ensure!(
        removed.problems.is_empty(),
        "{}",
        removed.problems.join("; ")
    );
    Ok(match removed.done.len() {
        0 => format!("{} had nothing to disconnect", agent.name),
        count => format!("disconnected {} · {count} thing(s) removed", agent.name),
    })
}

/// Install a skill an agent wrote, into every tool that can hold it.
async fn approveskill(name: &str, project: &str) -> Result<Vec<String>> {
    let home = crate::files::home()?;
    let receipts = crate::skill::Receipts::open(crate::files::database()?).await?;
    let skill = locateskill(name, project)?;
    let agents = crate::agent::agents(&home);
    let results = crate::skill::approve(&receipts, &agents, &skill, false).await?;
    Ok(results
        .into_iter()
        .filter_map(|(tool, outcome)| outcome.ok().map(|_| tool))
        .collect())
}

async fn rejectskill(name: &str, project: &str) -> Result<()> {
    let receipts = crate::skill::Receipts::open(crate::files::database()?).await?;
    let skill = locateskill(name, project)?;
    crate::skill::reject(&receipts, &skill).await.map(|_| ())
}

/// The skill a dashboard row names. The row carries the project rather than the
/// shelf, because a project root is the whole of what identifies one.
fn locateskill(name: &str, project: &str) -> Result<crate::skill::Skill> {
    let shelf = match project.is_empty() {
        true => crate::skill::Shelf::Global,
        false => crate::skill::Shelf::project(std::path::Path::new(project)),
    };
    crate::skill::library::read(&shelf, name)
}

async fn deletememory(state: &state::State, id: i64) -> Result<()> {
    let brain = crate::brain::Brain::open(&state.database).await?;
    brain
        .deletememory(id)
        .await?
        .map(|_| ())
        .context("that memory was already gone")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::{Memory, MemoryScope};
    use crate::tui::state::{Mode, PAGES, Page, Pending, State};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn sample() -> State {
        State {
            page: PAGES[0],
            mode: Mode::Browse,
            notice: Notice::Ready,
            quit: false,
            database: std::path::PathBuf::from("/tmp/brain.db"),
            stats: Default::default(),
            optimization: Default::default(),
            meshenabled: false,
            learnenabled: false,
            connections: Vec::new(),
            cli: crate::cli::InstallStatus::Missing,
            shell: None,
            guidance: None,
            memories: vec![Memory {
                id: 1,
                body: "the first line\nand a second".to_owned(),
                source: "review".to_owned(),
                scope: MemoryScope::Global,
                project: String::new(),
                created: 1_700_000_000,
                superseded: 0,
                abridged: false,
            }],
            query: String::new(),
            input: String::new(),
            agents: Vec::new(),
            workers: Vec::new(),
            mesherror: None,
            skills: Vec::new(),
            unmanaged: Vec::new(),
            vaults: Vec::new(),
            secrets: Vec::new(),
            backend: crate::vault::Backend::Encrypted,
            scope: None,
            cursor: [0; PAGES.len()],
        }
    }

    fn render(state: &State, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        terminal
            .draw(|frame| draw::frame(frame, state))
            .expect("draw");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    /// A panic inside `draw` happens with the terminal already in raw mode, so
    /// it does not just fail — it leaves the shell unusable. Every page is drawn
    /// at a normal size and at one far smaller than any layout was written for.
    #[test]
    fn every_page_draws_at_any_size_it_is_given() {
        for page in PAGES {
            for mode in [Mode::Browse, Mode::Naming] {
                for (width, height) in [(80, 24), (200, 60), (20, 8), (8, 4), (1, 1)] {
                    let mut state = sample();
                    state.page = page;
                    state.mode = mode.clone();
                    render(&state, width, height);
                }
            }
        }
    }

    /// The row past the end of the tool list is what makes the set of tools
    /// open-ended, so the cursor has to be able to reach it even when the
    /// machine has no tools at all.
    #[test]
    fn the_tool_list_always_has_a_row_for_adding_another() {
        let mut state = sample();
        state.page = Page::Connections;
        assert_eq!(state::rows(&state), 1);

        // On that row there is no tool to connect, so it asks for a name.
        let action = keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, Action::None));
        assert!(state.mode == Mode::Naming);
        render(&state, 80, 24);
    }

    #[test]
    fn a_name_that_cannot_be_a_file_is_refused_before_an_editor_opens() {
        let mut state = sample();
        state.page = Page::Connections;
        state.mode = Mode::Naming;
        for character in "My Tool".chars() {
            keys::handle(
                &mut state,
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            );
        }
        let action = keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, Action::None), "a space is not a file name");
        assert!(state.mode == Mode::Naming, "still asking");

        let mut state = sample();
        state.page = Page::Connections;
        state.mode = Mode::Naming;
        for character in "hermes".chars() {
            keys::handle(
                &mut state,
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            );
        }
        let action = keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, Action::Describe(name) if name == "hermes"));
        assert!(state.mode == Mode::Browse);
    }

    /// Disconnecting edits somebody else's configuration, so it asks first —
    /// and Enter, which is what a person presses to dismiss a prompt they did
    /// not read, is not the key that answers.
    #[test]
    fn disconnecting_a_tool_is_confirmed_first() {
        let mut state = sample();
        state.page = Page::Connections;
        state.connections.push(crate::tui::state::Connection {
            agent: crate::agent::tool::resolve(std::path::Path::new("/users/test"), None, "codex")
                .unwrap()
                .unwrap(),
            detection: crate::agent::Detection::missing(),
        });
        keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert!(matches!(state.mode, Mode::Confirm(Pending::Disconnect(_))));

        let action = keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(action, Action::None));
        assert!(state.mode == Mode::Browse);
    }

    #[test]
    fn the_help_overlay_draws_over_a_terminal_too_small_to_hold_it() {
        let mut state = sample();
        state.mode = Mode::Help;
        render(&state, 10, 3);
    }

    #[test]
    fn an_empty_list_takes_cursor_keys_without_moving_or_panicking() {
        let mut state = sample();
        state.memories.clear();
        state.page = Page::Memories;
        for code in [KeyCode::Down, KeyCode::Up, KeyCode::Char('G'), KeyCode::End] {
            keys::handle(&mut state, KeyEvent::new(code, KeyModifiers::NONE));
        }
        assert_eq!(state::cursor(&state), 0);
        render(&state, 80, 24);
    }

    #[test]
    fn only_y_confirms_a_delete_and_everything_else_abandons_it() {
        let mut state = sample();
        state.page = Page::Memories;
        state.mode = Mode::Confirm(Pending::DeleteMemory(7));
        let action = keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        );
        assert!(matches!(action, Action::DeleteMemory(7)));

        // Enter especially: it is what somebody presses to dismiss a prompt
        // they have not read, and it must not be the one that deletes.
        for code in [KeyCode::Enter, KeyCode::Char('d'), KeyCode::Esc] {
            let mut state = sample();
            state.mode = Mode::Confirm(Pending::DeleteMemory(7));
            let action = keys::handle(&mut state, KeyEvent::new(code, KeyModifiers::NONE));
            assert!(matches!(action, Action::None));
            assert!(matches!(state.mode, Mode::Browse));
        }
    }

    #[test]
    fn leaving_the_search_box_keeps_what_was_typed() {
        let mut state = sample();
        state.page = Page::Memories;
        keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        assert!(matches!(state.mode, Mode::Search));
        for character in "hel".chars() {
            keys::handle(
                &mut state,
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            );
        }
        keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        keys::handle(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(state.mode, Mode::Browse));
        assert_eq!(state.query, "he");
    }

    /// `q` is a search term as often as it is a command. While the box is open
    /// every printable key has to be a character, or the query can never
    /// contain one.
    #[test]
    fn typing_q_into_the_search_box_does_not_quit() {
        let mut state = sample();
        state.page = Page::Memories;
        state.mode = Mode::Search;
        keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );
        assert!(!state.quit);
        assert_eq!(state.query, "q");
    }

    /// The sidebar prints a heading when the group changes, so every page of a
    /// group has to be listed together — a stray one prints its heading twice
    /// and reads as two groups of the same name.
    #[test]
    fn pages_are_listed_grouped_so_no_heading_repeats() {
        let mut seen: Vec<&'static str> = Vec::new();
        for page in PAGES {
            let section = crate::tui::state::section(page);
            if seen.last() != Some(&section) {
                assert!(
                    !seen.contains(&section),
                    "`{section}` is split across the list: {seen:?} then {section}"
                );
                seen.push(section);
            }
        }
        assert!(seen.len() > 1, "the grouping does nothing with one group");
    }

    #[test]
    fn pages_wrap_in_both_directions() {
        let mut state = sample();
        keys::handle(
            &mut state,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE),
        );
        assert_eq!(state.page, PAGES[PAGES.len() - 1]);
        keys::handle(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.page, PAGES[0]);
    }
}

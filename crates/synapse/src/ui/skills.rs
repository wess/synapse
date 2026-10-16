//! The skill library, and the index of skills other people published.
//!
//! Two tabs, the way an IDE splits its plugins: what you have, and what you
//! could get. Search is a list down the left and the selected skill read in
//! full beside it, because the index says nothing about a skill but its name —
//! and a person deciding whether to bring somebody else's instructions into
//! every session should be reading those instructions, not a summary of them.

use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Entity, FontWeight, Hsla, Window, div, px};
use guise::prelude::*;
use synapsecore::skill::market::{Found, Preview, Searched};
use synapsecore::skill::{State, Status};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One library skill and where it stands in every connected tool.
#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub description: String,
    pub files: usize,
    pub places: Vec<Status>,
    /// `global` or `project`.
    pub scope: String,
    /// The project a project skill belongs to, empty otherwise.
    pub project: String,
    /// Written by an agent and not yet approved. It is in the library and in no
    /// tool, and stays there until somebody here says otherwise.
    pub proposed: bool,
    /// The one line the agent left saying why it wrote this, or for a skill
    /// added from the index, which one it was.
    pub note: String,
    /// What wrote the proposal: a tool, or the index.
    pub origin: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Installed,
    Search,
}

/// Where a search stands. `Idle` is a tab nobody has searched from yet, which
/// has to read differently from a search that found nothing.
#[derive(Clone, Debug, Default)]
pub enum Searching {
    #[default]
    Idle,
    Busy,
    Done(Searched),
    Failed(String),
}

/// The skill being read on the right of the search tab.
#[derive(Clone, Debug, Default)]
pub enum Reading {
    #[default]
    Nothing,
    Busy(String),
    Ready(Preview),
    Failed(String, String),
}

pub struct Search {
    pub query: Entity<TextInput>,
    pub searching: Searching,
    pub reading: Reading,
    pub adding: bool,
    /// Names already in the library, so a result can say so instead of
    /// offering an add that would be refused.
    pub library: Vec<String>,
    /// Where the search looks, drawn under the results.
    pub sources: AnyElement,
}

pub struct View {
    pub tab: Tab,
    pub search: Search,
    pub rows: Vec<Row>,
    /// Skills a tool has that the library does not, worth adopting.
    pub unmanaged: Vec<(String, String)>,
    pub problems: Vec<String>,
    pub folder: String,
    pub message: Option<(String, bool)>,
}

pub struct Actions {
    pub installed: Click,
    pub browse: Click,
    pub runsearch: Click,
    /// Read one result, by index id.
    pub select: Box<dyn Fn(String) -> Click>,
    /// Bring the skill being read into the library.
    pub add: Click,
    /// Open a page in the browser, by address.
    pub open: Box<dyn Fn(String) -> Click>,
    pub installall: Click,
    pub refresh: Click,
    pub openfolder: Click,
    /// Install one skill everywhere, by library name.
    pub install: Box<dyn Fn(String) -> Click>,
    /// Copy a tool's own skill into the library, by (tool, skill).
    pub adopt: Box<dyn Fn(String, String) -> Click>,
    /// Install a proposed skill and stop proposing it, by (skill, project).
    pub approve: Box<dyn Fn(String, String) -> Click>,
    /// Turn one down, which removes it, by (skill, project).
    pub reject: Box<dyn Fn(String, String) -> Click>,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        installed,
        browse,
        runsearch,
        select,
        add,
        open,
        installall,
        refresh,
        openfolder,
        install,
        adopt,
        approve,
        reject,
    } = actions;
    let empty = view.rows.is_empty();
    let waiting = view.rows.iter().filter(|row| row.proposed).count();
    let tab = view.tab;
    let count = view.rows.len();

    div()
        .id("skillsmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(980.0))
                .mx_auto()
                .px(px(34.0))
                .py(px(28.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(px(24.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(7.0))
                                .child(Title::new("Skills").order(2))
                                .child(
                                    Text::new(
                                        "One library, installed into every tool that reads the Agent Skills format. Edit a skill once here instead of keeping copies in step by hand.",
                                    )
                                    .size(Size::Sm)
                                    .dimmed(),
                                )
                                .when(waiting > 0, |element| {
                                    element.child(
                                        Text::new(format!(
                                            "{waiting} waiting for your review. A skill marked so is in no tool until you approve it.",
                                        ))
                                        .size(Size::Sm),
                                    )
                                }),
                        )
                        .when(tab == Tab::Installed, |element| element.child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    Button::new("skillsrefresh", "Refresh")
                                        .variant(Variant::Subtle)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .left_section(
                                            Icon::new(IconName::RefreshCw).size(Size::Xs),
                                        )
                                        .on_click(move |event, window, cx| {
                                            refresh(event, window, cx)
                                        }),
                                )
                                .child(
                                    Button::new("skillsinstallall", "Install all")
                                        .variant(Variant::Filled)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .disabled(empty)
                                        .left_section(
                                            Icon::new(IconName::Download).size(Size::Xs),
                                        )
                                        .on_click(move |event, window, cx| {
                                            installall(event, window, cx)
                                        }),
                                ),
                        )),
                )
                .child(tabs("skillstab", tab, count, installed, browse, cx))
                .when_some(view.message, |element, (message, error)| {
                    element.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(if error {
                                theme.danger().hsla()
                            } else {
                                theme.success().hsla()
                            })
                            .child(
                                Icon::new(if error {
                                    IconName::CircleAlert
                                } else {
                                    IconName::CircleCheck
                                })
                                .size(Size::Sm),
                            )
                            .child(Text::new(message).size(Size::Sm)),
                    )
                })
                .children(view.problems.into_iter().map(|problem| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_color(theme.danger().hsla())
                        .child(Icon::new(IconName::CircleAlert).size(Size::Xs))
                        .child(Text::new(format!("Skipped {problem}")).size(Size::Xs))
                        .into_any_element()
                }))
                .when(tab == Tab::Search, |element| {
                    element.children(search(view.search, runsearch, &select, add, &open, border, surface, cx))
                })
                .when(tab == Tab::Installed, |element| element.child(if empty {
                    blank(border, surface)
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .children(
                            view.rows.into_iter().enumerate().map(|(index, row)| {
                                skill(index, row, &install, &approve, &reject, border, surface)
                            }),
                        )
                        .into_any_element()
                }))
                .when(tab == Tab::Installed && !view.unmanaged.is_empty(), |element| {
                    element.child(unmanaged(view.unmanaged, &adopt, border, surface))
                })
                .when(tab == Tab::Installed, |element| element.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Text::new(view.folder).size(Size::Xs).dimmed())
                        .child(
                            Button::new("skillsfolder", "Open the library")
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| openfolder(event, window, cx)),
                        ),
                )),
        )
        .into_any_element()
}

/// The two tabs, drawn the way `guise::Tabs` draws them. Not that component
/// itself: it keeps its panels as closures inside an entity of its own, and
/// this page is a function of the dashboard's state like every other page.
/// Shared with the MCP page, which splits the same way.
pub fn tabs(
    prefix: &'static str,
    tab: Tab,
    count: usize,
    installed: Click,
    browse: Click,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let accent = theme.primary().hsla();
    let dimmed = theme.dimmed().hsla();
    let text = theme.text().hsla();
    let entry = |index: usize, label: String, active: bool, click: Click| {
        div()
            .id((prefix, index))
            .px(px(16.0))
            .py(px(8.0))
            .border_b_2()
            .border_color(if active {
                accent
            } else {
                gpui::transparent_black()
            })
            .text_size(px(13.0))
            .text_color(if active { accent } else { dimmed })
            .cursor_pointer()
            .hover(move |style| style.text_color(text))
            .child(label)
            .on_click(move |event, window, cx| click(event, window, cx))
    };
    div()
        .flex()
        .border_b_1()
        .border_color(theme.border().hsla())
        .child(entry(
            0,
            format!("Installed ({count})"),
            tab == Tab::Installed,
            installed,
        ))
        .child(entry(1, "Search".to_owned(), tab == Tab::Search, browse))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn search(
    view: Search,
    runsearch: Click,
    select: &dyn Fn(String) -> Click,
    add: Click,
    open: &dyn Fn(String) -> Click,
    border: Hsla,
    surface: Hsla,
    cx: &App,
) -> Vec<AnyElement> {
    let theme = guise::theme(cx);
    let selected = match &view.reading {
        Reading::Nothing => None,
        Reading::Busy(id) | Reading::Failed(id, _) => Some(id.clone()),
        Reading::Ready(preview) => Some(format!("{}:{}", preview.from, preview.id)),
    };
    let busy = matches!(view.searching, Searching::Busy);
    let bar = div()
        .flex()
        .items_end()
        .gap(px(8.0))
        // A column, so the field stretches to the width it is given rather
        // than shrinking to its placeholder.
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(view.query),
        )
        .child(
            Button::new("skillssearch", if busy { "Searching…" } else { "Search" })
                .variant(Variant::Filled)
                .color(ColorName::Violet)
                .size(Size::Sm)
                .disabled(busy)
                .left_section(Icon::new(IconName::Search).size(Size::Xs))
                .on_click(move |event, window, cx| runsearch(event, window, cx)),
        );
    let caveat = Text::new(
        "Every source below is asked at once. What you add waits for your approval before any tool gets it, and a skill can carry scripts.",
    )
    .size(Size::Xs)
    .dimmed();

    let results: AnyElement = match view.searching {
        Searching::Idle => note(
            "Search for what you want a skill to do",
            "Try “release notes”, “rust”, or “code review”.",
            border,
            surface,
        ),
        Searching::Busy => note("Searching every source…", "", border, surface),
        Searching::Failed(error) => div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(theme.danger().hsla())
            .child(Icon::new(IconName::CircleAlert).size(Size::Sm))
            .child(Text::new(error).size(Size::Sm))
            .into_any_element(),
        Searching::Done(searched) if searched.found.is_empty() => div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(problems(&searched.problems, theme.danger().hsla()))
            .child(note(
                "Nothing published matches that",
                "Fewer or broader words usually find something.",
                border,
                surface,
            ))
            .into_any_element(),
        Searching::Done(searched) => div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .children(problems(&searched.problems, theme.danger().hsla()))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(16.0))
                    .child(
                        div()
                            .w(px(300.0))
                            .flex_none()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(border)
                            .bg(surface)
                            .flex()
                            .flex_col()
                            .children(searched.found.into_iter().enumerate().map(
                                |(index, item)| {
                                    let active =
                                        selected.as_deref() == Some(item.qualified().as_str());
                                    let had = view.library.contains(&item.name);
                                    result(
                                        index,
                                        item,
                                        active,
                                        had,
                                        select,
                                        theme.primary().hsla(),
                                        border,
                                    )
                                },
                            )),
                    )
                    .child(div().flex_1().min_w(px(0.0)).child(detail(
                        view.reading,
                        view.adding,
                        &view.library,
                        add,
                        open,
                        border,
                        surface,
                        cx,
                    ))),
            )
            .into_any_element(),
    };

    // Children of the page column itself rather than of a column of their own:
    // a nested column lost its definite width, and then the longest unwrapped
    // line in a SKILL.md decided how wide the page was.
    vec![
        bar.into_any_element(),
        caveat.into_any_element(),
        results,
        view.sources,
    ]
}

/// What each source that could not answer said, above what the others found.
fn problems(problems: &[String], danger: Hsla) -> Vec<AnyElement> {
    problems
        .iter()
        .map(|problem| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(danger)
                .child(Icon::new(IconName::CircleAlert).size(Size::Xs))
                .child(Text::new(problem.clone()).size(Size::Xs))
                .into_any_element()
        })
        .collect()
}

fn result(
    index: usize,
    item: Found,
    active: bool,
    had: bool,
    select: &dyn Fn(String) -> Click,
    accent: Hsla,
    border: Hsla,
) -> AnyElement {
    let click = select(item.qualified());
    let detail = match item.installs {
        Some(count) => format!("{} · {}", item.repository, installs(count)),
        None => item.repository.clone(),
    };
    let from = item.from.clone();
    div()
        .id(("skillresult", index))
        .px(px(14.0))
        .py(px(10.0))
        .when(index > 0, |element| {
            element.border_t_1().border_color(border)
        })
        .border_l_2()
        .border_color(if active {
            accent
        } else {
            gpui::transparent_black()
        })
        .cursor_pointer()
        .hover(|style| style.opacity(0.85))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    Text::new(item.name)
                        .size(Size::Sm)
                        .weight(FontWeight::SEMIBOLD),
                )
                .when(had, |element| {
                    element.child(
                        Badge::new("in library")
                            .size(Size::Sm)
                            .color(ColorName::Teal),
                    )
                }),
        )
        .child(Text::new(detail).size(Size::Xs).dimmed())
        .child(
            div()
                .flex()
                .child(Badge::new(from).size(Size::Sm).color(ColorName::Gray)),
        )
        .on_click(move |event, window, cx| click(event, window, cx))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn detail(
    reading: Reading,
    adding: bool,
    library: &[String],
    add: Click,
    open: &dyn Fn(String) -> Click,
    border: Hsla,
    surface: Hsla,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let preview = match reading {
        Reading::Nothing => {
            return note(
                "Pick a skill to read it",
                "Its whole SKILL.md shows here before anything is kept.",
                border,
                surface,
            );
        }
        Reading::Busy(id) => return note(&format!("Reading {id}…"), "", border, surface),
        Reading::Failed(id, error) => {
            return note(&format!("Could not read {id}"), &error, border, surface);
        }
        Reading::Ready(preview) => preview,
    };
    let had = library.contains(&preview.name);
    let page = open(preview.page.clone());
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            Text::new(preview.name.clone())
                                .size(Size::Md)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            Text::new(preview.description.clone())
                                .size(Size::Xs)
                                .dimmed(),
                        )
                        .child(Text::new(preview.id.clone()).size(Size::Xs).dimmed()),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new("skillpage", preview.from.clone())
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::ExternalLink).size(Size::Xs))
                                .on_click(move |event, window, cx| page(event, window, cx)),
                        )
                        .child(if had {
                            Badge::new("in library")
                                .size(Size::Md)
                                .color(ColorName::Teal)
                                .into_any_element()
                        } else {
                            Button::new(
                                "skilladd",
                                if adding {
                                    "Adding…"
                                } else {
                                    "Add to library"
                                },
                            )
                            .variant(Variant::Filled)
                            .color(ColorName::Violet)
                            .size(Size::Xs)
                            .disabled(adding)
                            .left_section(Icon::new(IconName::Download).size(Size::Xs))
                            .on_click(move |event, window, cx| add(event, window, cx))
                            .into_any_element()
                        }),
                ),
        )
        .when(preview.files.len() > 1, |element| {
            element.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(preview.files.iter().map(|file| {
                        Badge::new(file.clone())
                            .size(Size::Sm)
                            .color(ColorName::Gray)
                            .into_any_element()
                    })),
            )
        })
        .child(
            div()
                .rounded(px(4.0))
                .border_1()
                .border_color(border)
                .bg(theme.body().hsla())
                .p(px(12.0))
                .font_family("Menlo")
                .text_size(px(12.0))
                .text_color(theme.text().hsla())
                .child(preview.entry),
        )
        .into_any_element()
}

fn note(title: &str, detail: &str, border: Hsla, surface: Hsla) -> AnyElement {
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(22.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(Text::new(title.to_owned()).size(Size::Sm).bold())
        .when(!detail.is_empty(), |element| {
            element.child(Text::new(detail.to_owned()).size(Size::Xs).dimmed())
        })
        .into_any_element()
}

/// `20133` as `20.1k installs`: a count at this scale is read for its size,
/// not its digits.
fn installs(count: u64) -> String {
    match count {
        0..1_000 => format!("{count} installs"),
        1_000..1_000_000 => format!("{:.1}k installs", count as f64 / 1_000.0),
        _ => format!("{:.1}M installs", count as f64 / 1_000_000.0),
    }
}

fn blank(border: gpui::Hsla, surface: gpui::Hsla) -> AnyElement {
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(22.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(Text::new("The library is empty").size(Size::Sm).bold())
        .child(
            Text::new(
                "Run `synapse skill create <name>` to start one, or `synapse skill adopt <name>` to bring in a skill a tool already has.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

fn skill(
    index: usize,
    row: Row,
    install: &dyn Fn(String) -> Click,
    approve: &dyn Fn(String, String) -> Click,
    reject: &dyn Fn(String, String) -> Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    let action = install(row.name.clone());
    let accept = approve(row.name.clone(), row.project.clone());
    let decline = reject(row.name.clone(), row.project.clone());
    let proposed = row.proposed;
    let project = row.project.clone();
    // An agent's note is its own words; a skill from the index has an address
    // instead, and quoting an address reads as if somebody said it.
    let note = match row.origin.strip_prefix(synapsecore::skill::market::ORIGIN) {
        Some(source) => format!("From {source} · {}", row.note),
        None if row.note.is_empty() => String::new(),
        None => format!("“{}”", row.note),
    };
    let scope = row.scope.clone();
    // A skill nobody has, or whose copies have fallen behind, is the one worth
    // acting on; the button says which.
    let pending = row
        .places
        .iter()
        .filter(|place| matches!(place.state, State::Missing | State::Stale))
        .count();
    let label = match pending {
        0 => "Reinstall".to_owned(),
        _ => format!("Install ({pending})"),
    };
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(18.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(9.0))
                                .child(
                                    Text::new(row.name.clone())
                                        .size(Size::Sm)
                                        .weight(FontWeight::SEMIBOLD),
                                )
                                .when(scope == "project", |element| {
                                    element.child(
                                        Badge::new("project").size(Size::Sm).color(ColorName::Blue),
                                    )
                                })
                                .when(proposed, |element| {
                                    element.child(
                                        Badge::new("waiting for review")
                                            .size(Size::Sm)
                                            .color(ColorName::Violet),
                                    )
                                })
                                .when(row.files > 1, |element| {
                                    element.child(
                                        Badge::new(format!("{} files", row.files))
                                            .size(Size::Sm)
                                            .color(ColorName::Gray),
                                    )
                                }),
                        )
                        .child(Text::new(row.description).size(Size::Xs).dimmed())
                        .when(!project.is_empty(), |element| {
                            element.child(Text::new(project.clone()).size(Size::Xs).dimmed())
                        })
                        .when(proposed && !note.is_empty(), |element| {
                            element.child(Text::new(note.clone()).size(Size::Xs).dimmed())
                        }),
                )
                .child(if proposed {
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new(("skillreject", index), "Turn down")
                                .variant(Variant::Subtle)
                                .color(ColorName::Red)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| decline(event, window, cx)),
                        )
                        .child(
                            Button::new(("skillapprove", index), "Approve")
                                .variant(Variant::Filled)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| accept(event, window, cx)),
                        )
                        .into_any_element()
                } else {
                    div()
                        .flex_none()
                        .child(
                            Button::new(("skillinstall", index), label)
                                .variant(Variant::Light)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| action(event, window, cx)),
                        )
                        .into_any_element()
                }),
        )
        // A proposed skill is in no tool by design, so a row of "not installed"
        // badges under it would read as something to fix.
        .when(!proposed, |element| {
            element.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(row.places.into_iter().map(place)),
            )
        })
        .into_any_element()
}

fn place(status: Status) -> AnyElement {
    let color = match status.state {
        State::Installed => ColorName::Teal,
        State::Stale => ColorName::Orange,
        State::Modified | State::Foreign => ColorName::Red,
        State::Missing => ColorName::Gray,
    };
    Badge::new(format!("{} · {}", status.tool, status.state.label()))
        .size(Size::Sm)
        .color(color)
        .into_any_element()
}

fn unmanaged(
    found: Vec<(String, String)>,
    adopt: &dyn Fn(String, String) -> Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(Text::new("Already in your tools").size(Size::Sm).bold())
                .child(
                    Text::new(
                        "Skills Synapse did not install. Adopt one to copy it into the library and keep it in step everywhere.",
                    )
                    .size(Size::Xs)
                    .dimmed(),
                ),
        )
        .children(found.into_iter().enumerate().map(|(index, (tool, name))| {
            let action = adopt(tool.clone(), name.clone());
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .child(Text::new(name.clone()).size(Size::Xs))
                        .child(Badge::new(tool.clone()).size(Size::Sm).color(ColorName::Gray)),
                )
                .child(
                    Button::new(("skilladopt", index), "Adopt")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| action(event, window, cx)),
                )
                .into_any_element()
        }))
        .into_any_element()
}

//! The MCP server library: what you have, and what the registry offers.
//!
//! The same split as Skills. Installed is one card per library server with a
//! badge per tool; Search is every MCP source as a list beside the
//! selected entry, read in full — the command it would run and everything it
//! still needs — before anything is kept.

use crate::ui::skills::{Click, Tab, tabs};
use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, FontWeight, Hsla, div, px};
use guise::prelude::*;
use synapsecore::servers::registry::{Listing, Searched};
use synapsecore::servers::{Holding, Placement};

#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub description: String,
    pub commandline: String,
    /// Secrets, settings, and `<placeholders>` still to fill in.
    pub missing: Vec<String>,
    /// Which of `missing` are secrets, so each gets a Set button.
    pub unset: Vec<String>,
    pub places: Vec<Placement>,
    /// The tool it was adopted from, whose own entry may be swapped for this.
    pub adoptedfrom: String,
}

#[derive(Clone, Debug, Default)]
pub enum Searching {
    #[default]
    Idle,
    Busy,
    Done(Searched),
    Failed(String),
}

pub struct Search {
    pub query: Entity<TextInput>,
    pub searching: Searching,
    pub selected: Option<usize>,
    pub adding: bool,
    pub library: Vec<String>,
    /// Where the search looks, drawn under the results.
    pub sources: AnyElement,
}

/// The one secret being typed, and where.
pub struct Entering {
    pub server: String,
    pub variable: String,
    pub input: Entity<PasswordInput>,
}

pub struct View {
    pub tab: Tab,
    pub rows: Vec<Row>,
    pub foreign: Vec<(String, String)>,
    pub problems: Vec<String>,
    pub folder: String,
    pub message: Option<(String, bool)>,
    pub busy: bool,
    pub search: Search,
    pub entering: Option<Entering>,
}

pub struct Actions {
    pub installed: Click,
    pub browse: Click,
    pub refresh: Click,
    pub openfolder: Click,
    pub install: Box<dyn Fn(String) -> Click>,
    pub uninstall: Box<dyn Fn(String) -> Click>,
    /// Swap the tool's own entry for the library's, by server name. Offered
    /// only for the tool a server was adopted from, whose entry the library
    /// already holds everything of.
    pub replace: Box<dyn Fn(String) -> Click>,
    pub edit: Box<dyn Fn(String) -> Click>,
    /// Start typing one secret, by (server, variable).
    pub asksecret: Box<dyn Fn(String, String) -> Click>,
    pub savesecret: Click,
    pub cancelsecret: Click,
    /// By (tool, server).
    pub adopt: Box<dyn Fn(String, String) -> Click>,
    pub runsearch: Click,
    pub select: Box<dyn Fn(usize) -> Click>,
    pub add: Click,
    pub open: Box<dyn Fn(String) -> Click>,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let tab = view.tab;
    let count = view.rows.len();
    let Actions {
        installed,
        browse,
        refresh,
        openfolder,
        install,
        uninstall,
        replace,
        edit,
        asksecret,
        savesecret,
        cancelsecret,
        adopt,
        runsearch,
        select,
        add,
        open,
    } = actions;
    let mut entering = view.entering;
    let mut savesecret = Some(savesecret);
    let mut cancelsecret = Some(cancelsecret);

    let header = div()
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
                .child(Title::new("MCP").order(2))
                .child(
                    Text::new(
                        "One list of MCP servers, given to every connected tool. Each tool starts a server through Synapse, which hands it its secrets from the vault, so no token sits in a tool's config.",
                    )
                    .size(Size::Sm)
                    .dimmed(),
                ),
        )
        .when(tab == Tab::Installed, |element| {
            element.child(
                div().flex_none().child(
                    Button::new("serversrefresh", "Refresh")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                        .on_click(move |event, window, cx| refresh(event, window, cx)),
                ),
            )
        });

    let mut page = div()
        .w_full()
        .max_w(px(980.0))
        .mx_auto()
        .px(px(34.0))
        .py(px(28.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(header)
        .child(tabs("serverstab", tab, count, installed, browse, cx))
        .when(view.busy, |element| {
            element.child(Text::new("Working…").size(Size::Sm).dimmed())
        })
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
        }));

    if tab == Tab::Search {
        page = page.children(search(
            view.search,
            runsearch,
            &select,
            add,
            &open,
            border,
            surface,
            cx,
        ));
    } else {
        if view.rows.is_empty() {
            page = page.child(note(
                "The library is empty",
                "Search the registry, adopt a server a tool already has, or run `synapse mcp add <name> -- <command>`.",
                border,
                surface,
            ));
        }
        for (index, row) in view.rows.into_iter().enumerate() {
            let typing = entering.take_if(|current| current.server == row.name);
            let card = card(
                index,
                row,
                typing,
                &mut savesecret,
                &mut cancelsecret,
                &install,
                &uninstall,
                &replace,
                &edit,
                &asksecret,
                border,
                surface,
            );
            page = page.child(card);
        }
        if !view.foreign.is_empty() {
            page = page.child(foreign(view.foreign, &adopt, border, surface));
        }
        page = page.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Text::new(view.folder).size(Size::Xs).dimmed())
                .child(
                    Button::new("serversfolder", "Open the library")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| openfolder(event, window, cx)),
                ),
        );
    }

    div()
        .id("serversmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(page)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn card(
    index: usize,
    row: Row,
    entering: Option<Entering>,
    savesecret: &mut Option<Click>,
    cancelsecret: &mut Option<Click>,
    install: &dyn Fn(String) -> Click,
    uninstall: &dyn Fn(String) -> Click,
    replace: &dyn Fn(String) -> Click,
    edit: &dyn Fn(String) -> Click,
    asksecret: &dyn Fn(String, String) -> Click,
    border: Hsla,
    surface: Hsla,
) -> AnyElement {
    let ready = row.missing.is_empty();
    let pending = row
        .places
        .iter()
        .filter(|place| matches!(place.holding, Holding::Missing | Holding::Stale))
        .count();
    let held = row
        .places
        .iter()
        .filter(|place| matches!(place.holding, Holding::Installed | Holding::Stale))
        .count();
    // A tool's own entry is only offered for swapping when the library holds
    // everything that entry had — which is true of the tool it was adopted
    // from and of no other.
    let swappable = !row.adoptedfrom.is_empty()
        && row
            .places
            .iter()
            .any(|place| place.tool == row.adoptedfrom && place.holding == Holding::Foreign);
    let label = match pending {
        0 => "Installed".to_owned(),
        _ => format!("Install ({pending})"),
    };
    let actions = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            Button::new(("serveredit", index), "Edit")
                .variant(Variant::Subtle)
                .color(ColorName::Violet)
                .size(Size::Xs)
                .on_click({
                    let click = edit(row.name.clone());
                    move |event, window, cx| click(event, window, cx)
                }),
        )
        .when(held > 0, |element| {
            let click = uninstall(row.name.clone());
            element.child(
                Button::new(("serveruninstall", index), "Remove from tools")
                    .variant(Variant::Subtle)
                    .color(ColorName::Red)
                    .size(Size::Xs)
                    .on_click(move |event, window, cx| click(event, window, cx)),
            )
        })
        .when(swappable, |element| {
            let click = replace(row.name.clone());
            element.child(
                Button::new(
                    ("serverreplace", index),
                    format!("Swap {}'s entry", row.adoptedfrom),
                )
                .variant(Variant::Light)
                .color(ColorName::Violet)
                .size(Size::Xs)
                .on_click(move |event, window, cx| click(event, window, cx)),
            )
        })
        .child({
            let click = install(row.name.clone());
            Button::new(("serverinstall", index), label)
                .variant(Variant::Filled)
                .color(ColorName::Violet)
                .size(Size::Xs)
                .disabled(!ready || pending == 0)
                .on_click(move |event, window, cx| click(event, window, cx))
        });

    let mut body = div()
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
                                .when(!ready, |element| {
                                    element.child(
                                        Badge::new("needs setup")
                                            .size(Size::Sm)
                                            .color(ColorName::Orange),
                                    )
                                }),
                        )
                        .when(!row.description.is_empty(), |element| {
                            element
                                .child(Text::new(row.description.clone()).size(Size::Xs).dimmed())
                        })
                        .child(Text::new(row.commandline.clone()).size(Size::Xs).dimmed()),
                )
                .child(actions),
        );

    if !ready {
        let mut needs = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(Text::new("Needs").size(Size::Xs).dimmed());
        for (position, variable) in row.missing.iter().enumerate() {
            if row.unset.contains(variable) {
                let click = asksecret(row.name.clone(), variable.clone());
                needs = needs.child(
                    Button::new(
                        ("serverneed", index * 100 + position),
                        format!("Set {variable}"),
                    )
                    .variant(Variant::Light)
                    .color(ColorName::Orange)
                    .size(Size::Xs)
                    .on_click(move |event, window, cx| click(event, window, cx)),
                );
            } else {
                needs = needs.child(
                    Badge::new(variable.clone())
                        .size(Size::Sm)
                        .color(ColorName::Orange),
                );
            }
        }
        body = body.child(needs);
    }

    if let Some(entering) = entering
        && let (Some(save), Some(cancel)) = (savesecret.take(), cancelsecret.take())
    {
        body = body.child(
            div()
                .flex()
                .items_end()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .child(entering.input),
                )
                .child(
                    Button::new("serversecretcancel", "Cancel")
                        .variant(Variant::Subtle)
                        .color(ColorName::Gray)
                        .size(Size::Sm)
                        .on_click(move |event, window, cx| cancel(event, window, cx)),
                )
                .child(
                    Button::new(
                        "serversecretsave",
                        format!("Save {} to the vault", entering.variable),
                    )
                    .variant(Variant::Filled)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .on_click(move |event, window, cx| save(event, window, cx)),
                ),
        );
    }

    if !row.places.is_empty() {
        body = body.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .children(row.places.into_iter().map(|place| {
                    let color = match place.holding {
                        Holding::Installed => ColorName::Teal,
                        Holding::Stale => ColorName::Orange,
                        Holding::Foreign => ColorName::Red,
                        Holding::Missing => ColorName::Gray,
                    };
                    Badge::new(format!("{} · {}", place.tool, place.holding.label()))
                        .size(Size::Sm)
                        .color(color)
                        .into_any_element()
                })),
        );
    }
    body.into_any_element()
}

fn foreign(
    found: Vec<(String, String)>,
    adopt: &dyn Fn(String, String) -> Click,
    border: Hsla,
    surface: Hsla,
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
                    div().flex().child(
                        div().flex_1().min_w(px(0.0)).child(
                            Text::new("Servers Synapse did not set up. Adopt one to move it into the library, its values into the vault.")
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                    ),
                ),
        )
        .children(found.into_iter().enumerate().map(|(index, (tool, name))| {
            let click = adopt(tool.clone(), name.clone());
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
                        .child(Text::new(name).size(Size::Xs))
                        .child(Badge::new(tool).size(Size::Sm).color(ColorName::Gray)),
                )
                .child(
                    Button::new(("serveradopt", index), "Adopt")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| click(event, window, cx)),
                )
                .into_any_element()
        }))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn search(
    view: Search,
    runsearch: Click,
    select: &dyn Fn(usize) -> Click,
    add: Click,
    open: &dyn Fn(String) -> Click,
    border: Hsla,
    surface: Hsla,
    cx: &App,
) -> Vec<AnyElement> {
    let theme = guise::theme(cx);
    let busy = matches!(view.searching, Searching::Busy);
    let bar = div()
        .flex()
        .items_end()
        .gap(px(8.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(view.query),
        )
        .child(
            Button::new("serverssearch", if busy { "Searching…" } else { "Search" })
                .variant(Variant::Filled)
                .color(ColorName::Violet)
                .size(Size::Sm)
                .disabled(busy)
                .left_section(Icon::new(IconName::Search).size(Size::Xs))
                .on_click(move |event, window, cx| runsearch(event, window, cx)),
        );
    // Kept to one line: a paragraph alone in its row lays out wider than the
    // page in this window.
    let caveat = Text::new(
        "Every source below is asked at once; a registry can take half a minute. Adding one changes no tool until you install it.",
    )
    .size(Size::Xs)
    .dimmed();

    let results: AnyElement = match view.searching {
        Searching::Idle => note(
            "Search for a service or a tool",
            "Try “github”, “postgres”, or “filesystem”.",
            border,
            surface,
        ),
        Searching::Busy => note(
            "Asking every source…",
            "It is slow; this can take half a minute.",
            border,
            surface,
        ),
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
                "No source has a server matching that",
                "A registry matches on names, so try the service's name.",
                border,
                surface,
            ))
            .into_any_element(),
        Searching::Done(searched) => {
            let found = searched.found;
            let chosen = view.selected.and_then(|index| found.get(index).cloned());
            div()
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
                                .overflow_hidden()
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface)
                                .flex()
                                .flex_col()
                                .children(found.into_iter().enumerate().map(|(index, listing)| {
                                    let click = select(index);
                                    let active = view.selected == Some(index);
                                    let installable = listing.draft.is_some();
                                    div()
                                        .id(("serverresult", index))
                                        .px(px(14.0))
                                        .py(px(10.0))
                                        .when(index > 0, |element| {
                                            element.border_t_1().border_color(border)
                                        })
                                        .border_l_2()
                                        .border_color(if active {
                                            theme.primary().hsla()
                                        } else {
                                            gpui::transparent_black()
                                        })
                                        .cursor_pointer()
                                        .hover(|style| style.opacity(0.85))
                                        .flex()
                                        .flex_col()
                                        .gap(px(3.0))
                                        .child(
                                            Text::new(match listing.title.is_empty() {
                                                true => listing
                                                    .id
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(&listing.id)
                                                    .to_owned(),
                                                false => listing.title.clone(),
                                            })
                                            .size(Size::Sm)
                                            .weight(FontWeight::SEMIBOLD),
                                        )
                                        .child(
                                            Text::new(listing.id.clone()).size(Size::Xs).dimmed(),
                                        )
                                        // In a row, so each badge is as wide as its
                                        // words rather than the whole column.
                                        .child(
                                            div()
                                                .flex()
                                                .gap(px(6.0))
                                                .child(
                                                    Badge::new(listing.from.clone())
                                                        .size(Size::Sm)
                                                        .color(ColorName::Gray),
                                                )
                                                .when(!installable, |element| {
                                                    element.child(
                                                        Badge::new("can't run here")
                                                            .size(Size::Sm)
                                                            .color(ColorName::Gray),
                                                    )
                                                }),
                                        )
                                        .on_click(move |event, window, cx| click(event, window, cx))
                                        .into_any_element()
                                })),
                        )
                        .child(div().flex_1().min_w(px(0.0)).child(detail(
                            chosen,
                            view.adding,
                            &view.library,
                            add,
                            open,
                            border,
                            surface,
                            cx,
                        ))),
                )
                .into_any_element()
        }
    };
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

#[allow(clippy::too_many_arguments)]
fn detail(
    chosen: Option<Listing>,
    adding: bool,
    library: &[String],
    add: Click,
    open: &dyn Fn(String) -> Click,
    border: Hsla,
    surface: Hsla,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let Some(listing) = chosen else {
        return note(
            "Pick a server to read it",
            "What it would run shows here.",
            border,
            surface,
        );
    };
    let had = listing
        .draft
        .as_ref()
        .is_some_and(|draft| library.contains(&draft.name));
    let mut panel = div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0));
    // Always present, even with nothing in it: a heading with nothing fixed
    // beside it lays out wider than the panel in this window.
    let mut buttons = div()
        .flex_none()
        .min_w(px(1.0))
        .flex()
        .items_center()
        .gap(px(8.0));
    if !listing.repository.is_empty() {
        let click = open(listing.repository.clone());
        buttons = buttons.child(
            Button::new("serverrepository", "Source")
                .variant(Variant::Subtle)
                .color(ColorName::Violet)
                .size(Size::Xs)
                .left_section(Icon::new(IconName::ExternalLink).size(Size::Xs))
                .on_click(move |event, window, cx| click(event, window, cx)),
        );
    }
    if listing.draft.is_some() {
        buttons = buttons.child(if had {
            Badge::new("in library")
                .size(Size::Md)
                .color(ColorName::Teal)
                .into_any_element()
        } else {
            Button::new(
                "serveradd",
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
        });
    }
    panel = panel.child(
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
                        Text::new(listing.id.clone())
                            .size(Size::Md)
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        Text::new(listing.description.clone())
                            .size(Size::Xs)
                            .dimmed(),
                    )
                    .child(
                        Text::new(format!("Version {}", listing.version))
                            .size(Size::Xs)
                            .dimmed(),
                    ),
            )
            .child(buttons),
    );
    if let Some(draft) = &listing.draft {
        panel = panel.child(
            div()
                .rounded(px(4.0))
                .border_1()
                .border_color(border)
                .bg(theme.body().hsla())
                .p(px(12.0))
                .font_family("Menlo")
                .text_size(px(12.0))
                .text_color(theme.text().hsla())
                .child(draft.commandline()),
        );
    }
    if !listing.note.is_empty() {
        panel = panel.child(
            div().flex().child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Text::new(listing.note.clone()).size(Size::Xs)),
            ),
        );
    }
    panel.into_any_element()
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

//! The left column: where you are, and everywhere else you could be.
//!
//! Seven destinations in a row across the top was already crowded, and the
//! console makes it worse — a nav that reflows when the window narrows is a nav
//! that moves the thing you were about to click. Down the side it has room to
//! grow, room to say what each group is for, and it stops competing with the
//! page's own title for the top of the window.

use crate::ui::Page;
use gpui::prelude::*;
use gpui::{App, ClickEvent, Entity, Hsla, IntoElement, SharedString, Window, div, px};
use guise::prelude::*;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Wide enough for the longest label beside its icon, narrow enough that the
/// page keeps the window.
const WIDTH: f32 = 212.0;

/// Where each navigation button goes. Named rather than passed in a row,
/// because seven handlers of one type in sequence is seven chances to wire a
/// button to the wrong page and have nothing about it look wrong.
pub struct Navigation {
    pub connections: Click,
    pub memories: Click,
    pub skills: Click,
    pub console: Click,
    pub mesh: Click,
    pub vaults: Click,
    pub settings: Click,
}

pub fn render(
    page: Page,
    appmenu: Option<Entity<MenuBar>>,
    go: Navigation,
    cx: &App,
) -> impl IntoElement {
    let Navigation {
        connections,
        memories,
        skills,
        console,
        mesh,
        vaults,
        settings,
    } = go;
    let theme = guise::theme(cx);
    let colours = Colours {
        text: theme.text().hsla(),
        dim: theme.dimmed().hsla(),
        hover: theme.surface_hover().hsla(),
    };
    div()
        .flex_none()
        .w(px(WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .px(px(12.0))
        .py(px(14.0))
        .gap(px(2.0))
        .border_r_1()
        .border_color(theme.border().hsla())
        .bg(theme.surface().hsla())
        .child(wordmark())
        // Grouped because the three answer different questions: what is Synapse
        // wired into, what is it holding, and what is running right now.
        .child(group("Workspace"))
        .child(entry(
            "navconnections",
            "Connections",
            IconName::Plug,
            page == Page::Connections,
            &colours,
            connections,
        ))
        .child(entry(
            "navmemories",
            "Memory",
            IconName::Brain,
            page == Page::Memories,
            &colours,
            memories,
        ))
        .child(entry(
            "navskills",
            "Skills",
            IconName::Sparkles,
            page == Page::Skills,
            &colours,
            skills,
        ))
        .child(group("Agents"))
        .child(entry(
            "navconsole",
            "Console",
            IconName::MessageSquare,
            page == Page::Console,
            &colours,
            console,
        ))
        .child(entry(
            "navmesh",
            "Mesh",
            IconName::Waypoints,
            page == Page::Mesh,
            &colours,
            mesh,
        ))
        .child(group("System"))
        .child(entry(
            "navvaults",
            "Vaults",
            IconName::KeyRound,
            page == Page::Vaults,
            &colours,
            vaults,
        ))
        .child(entry(
            "navsettings",
            "Settings",
            IconName::Settings,
            page == Page::Settings,
            &colours,
            settings,
        ))
        // Pushes whatever follows to the bottom of the column.
        .child(div().flex_1())
        .when_some(appmenu, |element, menu| element.child(menu))
}

fn wordmark() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(9.0))
        .px(px(4.0))
        .pb(px(14.0))
        .child(
            div()
                .size(px(30.0))
                .rounded(px(9.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(guise::rgb(103, 82, 176))
                .text_color(guise::rgb(255, 255, 255))
                .child(Icon::new(IconName::BrainCircuit).size(Size::Sm)),
        )
        .child(Title::new("Synapse").order(4))
}

/// A heading over a run of destinations. Small, quiet, and not clickable —
/// a group that looked like a row would be a row that does nothing.
fn group(label: &'static str) -> impl IntoElement {
    div()
        .px(px(8.0))
        .pt(px(12.0))
        .pb(px(4.0))
        .child(Text::new(label.to_uppercase()).size(Size::Xs).dimmed())
}

/// One destination.
///
/// Built by hand rather than from a `Button`, because a button fills its width
/// by centring what is in it, and a centred row in a column of rows reads as a
/// heading. Left-aligned with the icon in a fixed gutter is what makes seven of
/// these scan as a list.
struct Colours {
    text: Hsla,
    dim: Hsla,
    hover: Hsla,
}

fn entry(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    here: bool,
    colours: &Colours,
    go: Click,
) -> impl IntoElement {
    let (text, hover) = (colours.text, colours.hover);
    div()
        .id(SharedString::new_static(id))
        .flex()
        .items_center()
        .gap(px(9.0))
        .w_full()
        .h(px(32.0))
        .px(px(8.0))
        .rounded(px(7.0))
        .when(here, |element| element.bg(hover))
        .text_color(if here { colours.text } else { colours.dim })
        .hover(move |style| style.bg(hover).text_color(text))
        .child(Icon::new(icon).size(Size::Xs))
        .child(Text::new(label).size(Size::Sm))
        .on_click(move |event, window, cx| go(event, window, cx))
}

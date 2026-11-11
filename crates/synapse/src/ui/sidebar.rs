//! The left column: where you are, and everywhere else you could be.
//!
//! Seven destinations in a row across the top is crowded, and a nav that
//! reflows when the window narrows is a nav that moves the thing you were about
//! to click. Down the side it has room to
//! grow, room to say what each group is for, and it stops competing with the
//! page's own title for the top of the window.

use crate::ui::Page;
use gpui::prelude::*;
use gpui::{App, ClickEvent, Hsla, IntoElement, SharedString, Window, div, px};
use guise::prelude::*;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Wide enough for the longest label beside its icon, narrow enough that the
/// page keeps the window.
const WIDTH: f32 = 196.0;

/// Where each navigation button goes. Named rather than passed in a row,
/// because a run of handlers with one type is easy to wire to the wrong page
/// and have nothing about it look wrong.
pub struct Navigation {
    pub map: Click,
    pub connections: Click,
    pub stats: Click,
    pub memories: Click,
    pub skills: Click,
    pub servers: Click,
    pub mesh: Click,
    pub logs: Click,
    pub vaults: Click,
    pub settings: Click,
}

pub fn render(page: Page, go: Navigation, cx: &App) -> impl IntoElement {
    let Navigation {
        map,
        connections,
        stats,
        memories,
        skills,
        servers,
        mesh,
        logs,
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
        .px(px(10.0))
        .py(px(12.0))
        .gap(px(1.0))
        .border_r_1()
        .border_color(theme.border().hsla())
        .bg(theme.surface().hsla())
        .child(wordmark())
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
            "navstats",
            "Usage",
            IconName::ChartColumn,
            page == Page::Stats,
            &colours,
            stats,
        ))
        .child(group("Explore"))
        .child(entry(
            "navmap",
            "Memory Map",
            IconName::ChartNetwork,
            page == Page::Map,
            &colours,
            map,
        ))
        .child(group("Extensions"))
        .child(entry(
            "navskills",
            "Skills",
            IconName::Sparkles,
            page == Page::Skills,
            &colours,
            skills,
        ))
        .child(entry(
            "navservers",
            "Tool Servers",
            IconName::Server,
            page == Page::Servers,
            &colours,
            servers,
        ))
        .child(group("Coordination"))
        .child(entry(
            "navmesh",
            "Agents",
            IconName::Waypoints,
            page == Page::Mesh,
            &colours,
            mesh,
        ))
        .child(entry(
            "navlogs",
            "Logs",
            IconName::Logs,
            page == Page::Logs,
            &colours,
            logs,
        ))
        .child(group("Security"))
        .child(entry(
            "navvaults",
            "Secrets",
            IconName::KeyRound,
            page == Page::Vaults,
            &colours,
            vaults,
        ))
        .child(div().flex_1())
        .child(entry(
            "navsettings",
            "Settings",
            IconName::Settings,
            page == Page::Settings,
            &colours,
            settings,
        ))
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
                .size(px(26.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(guise::rgb(103, 82, 176))
                .text_color(guise::rgb(255, 255, 255))
                .child(Icon::new(IconName::BrainCircuit).size(Size::Sm)),
        )
        .child(Text::new("Synapse").size(Size::Md).bold())
}

/// A heading over a run of destinations. Small, quiet, and not clickable —
/// a group that looked like a row would be a row that does nothing.
fn group(label: &'static str) -> impl IntoElement {
    div()
        .px(px(8.0))
        .pt(px(12.0))
        .pb(px(4.0))
        .child(Text::new(label).size(Size::Xs).dimmed())
}

/// One destination.
///
/// Built by hand rather than from a `Button`, because a button fills its width
/// by centring what is in it, and a centred row in a column of rows reads as a
/// heading. Left-aligned with the icon in a fixed gutter is what makes eight of
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
        .focusable()
        .tab_index(0)
        .when(here, |element| element.bg(hover))
        .text_color(if here { colours.text } else { colours.dim })
        .hover(move |style| style.bg(hover).text_color(text))
        .child(Icon::new(icon).size(Size::Xs))
        .child(Text::new(label).size(Size::Sm))
        .on_click(move |event, window, cx| go(event, window, cx))
}

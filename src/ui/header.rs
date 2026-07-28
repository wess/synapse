use crate::ui::Page;
use gpui::prelude::*;
use gpui::{App, ClickEvent, Entity, IntoElement, Window, div, px};
use guise::prelude::*;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn render(
    page: Page,
    appmenu: Option<Entity<MenuBar>>,
    connections: Click,
    memories: Click,
    vaults: Click,
    settings: Click,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    div()
        .flex_none()
        .h(px(74.0))
        .flex()
        .items_center()
        .justify_between()
        .px(px(30.0))
        .border_b_1()
        .border_color(theme.border().hsla())
        .bg(theme.surface().hsla())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(11.0))
                .child(
                    div()
                        .size(px(36.0))
                        .rounded(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(guise::rgb(103, 82, 176))
                        .text_color(guise::rgb(255, 255, 255))
                        .child(Icon::new(IconName::BrainCircuit).size(Size::Md)),
                )
                .child(Title::new("Synapse").order(3))
                .when_some(appmenu, |element, menu| {
                    element.child(div().ml(px(16.0)).child(menu))
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    Button::new("connectionsnav", "Connections")
                        .variant(if page == Page::Connections {
                            Variant::Light
                        } else {
                            Variant::Subtle
                        })
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| connections(event, window, cx)),
                )
                .child(
                    Button::new("memoriesnav", "Memory")
                        .variant(if page == Page::Memories {
                            Variant::Light
                        } else {
                            Variant::Subtle
                        })
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::Brain).size(Size::Xs))
                        .on_click(move |event, window, cx| memories(event, window, cx)),
                )
                .child(
                    Button::new("vaultsnav", "Vaults")
                        .variant(if page == Page::Vaults {
                            Variant::Light
                        } else {
                            Variant::Subtle
                        })
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::KeyRound).size(Size::Xs))
                        .on_click(move |event, window, cx| vaults(event, window, cx)),
                )
                .child(
                    Button::new("settingsnav", "Settings")
                        .variant(if page == Page::Settings {
                            Variant::Light
                        } else {
                            Variant::Subtle
                        })
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::Settings).size(Size::Xs))
                        .on_click(move |event, window, cx| settings(event, window, cx)),
                )
                .child(clibadge()),
        )
}

fn clibadge() -> impl IntoElement {
    let (label, color) = match crate::cli::status() {
        Ok(crate::cli::InstallStatus::Installed(_)) => ("CLI ready", ColorName::Teal),
        Ok(crate::cli::InstallStatus::Conflict(_)) => ("CLI conflict", ColorName::Orange),
        _ => ("On device", ColorName::Teal),
    };
    Badge::new(label).color(color).variant(Variant::Light)
}

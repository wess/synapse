use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Window, div, px};
use guise::prelude::*;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct Actions {
    pub install: Click,
    pub later: Click,
    pub never: Click,
}

pub fn render(path: String, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let install = actions.install;
    let later = actions.later;
    let never = actions.never;
    div()
        .w_full()
        .max_w(px(980.0))
        .mx_auto()
        .px(px(34.0))
        .pt(px(20.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(theme.border().hsla())
                .bg(theme.surface().hsla())
                .p(px(16.0))
                // A flex child sizes to its content by default, and an install
                // path is one long unbreakable run, so without a zero minimum
                // this column refuses to shrink and the text leaves the card.
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex_none()
                                .child(Icon::new(IconName::Terminal).size(Size::Sm)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    Text::new("Install the synapse command line tool?")
                                        .size(Size::Sm)
                                        .bold(),
                                )
                                .child(
                                    Text::new(format!(
                                        "Adds synapse to {path} for shell integration and scoped commands."
                                    ))
                                    .size(Size::Xs)
                                    .dimmed(),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new("clibannerinstall", "Install CLI")
                                .variant(Variant::Light)
                                .color(ColorName::Violet)
                                .size(Size::Sm)
                                .left_section(Icon::new(IconName::Download).size(Size::Xs))
                                .on_click(move |event, window, cx| install(event, window, cx)),
                        )
                        .child(
                            Button::new("clibannerlater", "Not now")
                                .variant(Variant::Subtle)
                                .size(Size::Sm)
                                .on_click(move |event, window, cx| later(event, window, cx)),
                        )
                        .child(
                            Button::new("clibannernever", "Don't show again")
                                .variant(Variant::Subtle)
                                .size(Size::Sm)
                                .on_click(move |event, window, cx| never(event, window, cx)),
                        ),
                ),
        )
        .into_any_element()
}

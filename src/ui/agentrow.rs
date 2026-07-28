use crate::ui::Row;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, px};
use guise::prelude::*;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn render(
    index: usize,
    row: Row,
    onset: Click,
    oninstructions: Click,
    onsettings: Click,
) -> AnyElement {
    let installed = row.detection.executable.is_some();
    let connected = row.detection.configured;
    let status = if connected {
        ("Connected", ColorName::Teal)
    } else if installed {
        ("Detected", ColorName::Blue)
    } else {
        ("Not installed", ColorName::Gray)
    };
    let detail = row
        .detection
        .version
        .unwrap_or_else(|| "Command not found on PATH".to_owned());

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .min_h(px(92.0))
        .px(px(22.0))
        .py(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .min_w(px(260.0))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(42.0))
                        .rounded(px(10.0))
                        .bg(guise::rgba(103, 82, 176, 0.1))
                        .text_color(guise::rgb(83, 62, 155))
                        .child(Icon::new(IconName::SquareTerminal).size(Size::Md)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(9.0))
                                .child(
                                    Text::new(row.agent.name)
                                        .size(Size::Sm)
                                        .weight(FontWeight::SEMIBOLD),
                                )
                                .child(Badge::new(status.0).size(Size::Sm).color(status.1)),
                        )
                        .child(Text::new(detail).size(Size::Xs).dimmed()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(
                    Button::new(("instructions", index), "Edit instructions")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::FileText).size(Size::Xs))
                        .on_click(move |event, window, cx| oninstructions(event, window, cx)),
                )
                .child(
                    Button::new(("settings", index), "Edit config")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::Settings2).size(Size::Xs))
                        .on_click(move |event, window, cx| onsettings(event, window, cx)),
                )
                .child(
                    Button::new(
                        ("setup", index),
                        if connected { "Connected" } else { "Set up" },
                    )
                    .variant(if connected {
                        Variant::Light
                    } else {
                        Variant::Filled
                    })
                    .size(Size::Xs)
                    .color(ColorName::Violet)
                    .disabled(!installed || connected)
                    .left_section(Icon::new(if connected {
                        IconName::CircleCheck
                    } else {
                        IconName::PlugZap
                    }))
                    .on_click(move |event, window, cx| onset(event, window, cx)),
                ),
        )
        .into_any_element()
}

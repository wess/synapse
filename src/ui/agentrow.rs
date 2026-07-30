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
    onnotice: Click,
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
    // Only Claude Code can state the connection at startup, so only its row
    // offers the control and reports what the tool's own settings currently say.
    let hooks = row.detection.hooks;
    let notice = (connected && row.agent.kind == crate::agent::Kind::Claude).then(|| {
        let summary = if hooks.notice {
            match (hooks.statusline, hooks.borrowed) {
                (true, _) => "Announces Synapse at startup · status line on".to_owned(),
                (_, true) => "Announces Synapse at startup · your status line kept".to_owned(),
                _ => "Announces Synapse at startup".to_owned(),
            }
        } else {
            "Does not announce Synapse at startup".to_owned()
        };
        (summary, hooks.notice)
    });

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
                        .child(Text::new(detail).size(Size::Xs).dimmed())
                        .when_some(notice, |element, (summary, installed)| {
                            element.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Icon::new(IconName::Sparkles).size(Size::Xs))
                                    .child(Text::new(summary).size(Size::Xs).dimmed())
                                    .child(
                                        Button::new(
                                            ("notice", index),
                                            if installed { "Remove" } else { "Add" },
                                        )
                                        .variant(Variant::Subtle)
                                        .color(ColorName::Violet)
                                        .size(Size::Xs)
                                        .on_click(
                                            move |event, window, cx| onnotice(event, window, cx),
                                        ),
                                    ),
                            )
                        }),
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

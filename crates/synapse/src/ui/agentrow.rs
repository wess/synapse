use crate::ui::Row;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, px};
use guise::prelude::*;

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Everything a row can be asked to do. A struct rather than seven positional
/// arguments, which was already one too many to read at the call site and is
/// where a mix-up would be silent — every one of them has the same type.
pub struct Actions {
    /// Connect a tool that is not connected.
    pub set: Click,
    /// Apply this release's descriptor to one that is.
    pub update: Click,
    /// Disconnect and connect again.
    pub reset: Click,
    /// Disconnect and leave it that way.
    pub remove: Click,
    pub instructions: Click,
    pub settings: Click,
    pub notice: Click,
    pub descriptor: Click,
}

pub fn render(index: usize, row: Row, actions: Actions) -> AnyElement {
    let installed = row.installed();
    let connected = row.connected();
    let outdated = row.outdated;
    let status = if outdated {
        // Connected, and this release would connect it differently. Amber
        // rather than red: nothing is broken, there is just something newer to
        // apply, and colouring it as a fault would teach people to ignore it.
        ("Update available", ColorName::Orange)
    } else if connected {
        ("Connected", ColorName::Teal)
    } else if installed {
        ("Detected", ColorName::Blue)
    } else {
        ("Not installed", ColorName::Gray)
    };
    let Actions {
        set: onset,
        update: onupdate,
        reset: onreset,
        remove: onremove,
        instructions: oninstructions,
        settings: onsettings,
        notice: onnotice,
        descriptor: ondescriptor,
    } = actions;
    let detail = row
        .detection
        .version
        .unwrap_or_else(|| "Command not found on PATH".to_owned());
    // Only Claude Code can state the connection at startup, so only its row
    // offers the control and reports what the tool's own settings currently say.
    let hooks = row.detection.hooks;
    let notice = (connected && row.agent.kind == synapsecore::agent::Kind::Claude).then(|| {
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

    // Identity above controls, in a column, rather than the two side by side.
    //
    // They used to share one wrapping line, which held while a row carried four
    // controls: at the width the page is capped to, a connected row now carries
    // six, the line wraps whether the window is wide or not, and a wrapped line
    // lands against the *next* tool's name — so every group of buttons read as
    // belonging to the row below it. A column cannot do that at any width, and
    // the controls were already dropping to their own line in practice.
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .min_w(px(0.0))
        .px(px(22.0))
        .py(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.0))
                .min_w(px(0.0))
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
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
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
                                    .flex_wrap()
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
            // Still wraps, but now only against itself: on a narrow window the
            // controls stack among themselves instead of colliding with a name.
            div()
                .flex()
                .flex_wrap()
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
                    Button::new(("descriptor", index), "Descriptor")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::PlugZap).size(Size::Xs))
                        .on_click(move |event, window, cx| ondescriptor(event, window, cx)),
                )
                .child(
                    Button::new(("settings", index), "Edit config")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .left_section(Icon::new(IconName::Settings2).size(Size::Xs))
                        .on_click(move |event, window, cx| onsettings(event, window, cx)),
                )
                // A connected row carries what you can do to the connection;
                // an unconnected one carries the one thing worth doing to it.
                // The badge already says "Connected", so a disabled button
                // repeating it was a control that could never be pressed.
                .when(connected, |element| {
                    element
                        .child(
                            Button::new(("update", index), "Update")
                                .variant(if outdated {
                                    Variant::Filled
                                } else {
                                    Variant::Subtle
                                })
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                                .on_click(move |event, window, cx| onupdate(event, window, cx)),
                        )
                        .child(
                            Button::new(("reset", index), "Reset")
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::RotateCcw).size(Size::Xs))
                                .on_click(move |event, window, cx| onreset(event, window, cx)),
                        )
                        .child(
                            Button::new(("remove", index), "Remove")
                                .variant(Variant::Subtle)
                                .color(ColorName::Red)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::Unplug).size(Size::Xs))
                                .on_click(move |event, window, cx| onremove(event, window, cx)),
                        )
                })
                .when(!connected, |element| {
                    element.child(
                        Button::new(("setup", index), "Set up")
                            .variant(Variant::Filled)
                            .size(Size::Xs)
                            .color(ColorName::Violet)
                            .disabled(!installed)
                            .left_section(Icon::new(IconName::PlugZap))
                            .on_click(move |event, window, cx| onset(event, window, cx)),
                    )
                }),
        )
        .into_any_element()
}

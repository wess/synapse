use crate::brain::Stats;
use crate::ui::Notice;
use gpui::prelude::*;
use gpui::{App, IntoElement, div, px};
use guise::prelude::*;

pub fn render(
    stats: &Stats,
    connected: usize,
    total: usize,
    notice: &Notice,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let noticetext = match notice {
        Notice::Ready => theme.dimmed(),
        Notice::Success(_) => theme.success(),
        Notice::Error(_) => theme.danger(),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(18.0))
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .gap(px(32.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .max_w(px(590.0))
                        .child(Title::new("Connect your coding tools").order(2))
                        .child(
                            Text::new(
                                "One memory, available wherever you work. Setup preserves your existing global instructions and settings.",
                            )
                            .size(Size::Sm)
                            .dimmed(),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(18.0))
                        .child(metric("Memories", stats.entries.to_string()))
                        .child(metric("Database", formatsize(stats.bytes)))
                        .child(metric("Connected", format!("{connected}/{total}"))),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(9.0))
                .min_h(px(32.0))
                .px(px(2.0))
                .text_color(noticetext.hsla())
                .child(
                    Icon::new(match notice {
                        Notice::Error(_) => IconName::CircleAlert,
                        Notice::Success(_) => IconName::CircleCheck,
                        Notice::Ready => IconName::ShieldCheck,
                    })
                    .size(Size::Sm),
                )
                .child(
                    Text::new(notice.message().to_owned())
                        .size(Size::Sm)
                        .color(noticetext),
                ),
        )
}

fn metric(label: &str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(Text::new(value).size(Size::Sm).bold())
        .child(Text::new(label.to_owned()).size(Size::Xs).dimmed())
}

fn formatsize(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

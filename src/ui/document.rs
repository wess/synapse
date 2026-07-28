use crate::ui::buffer::{self, Buffer, Format};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Window, actions, div, px};
use guise::prelude::*;
use std::path::PathBuf;

actions!(synaps, [SaveDocument]);

#[derive(Clone)]
pub struct Document {
    pub tool: String,
    pub path: PathBuf,
    pub editor: Buffer,
    pub format: Format,
    pub saved: String,
    pub current: String,
    pub error: Option<String>,
}

type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn dirty(document: &Document) -> bool {
    document.current != document.saved
}

pub fn render(
    document: Document,
    onsave: Click,
    onclose: Click,
    ondiscard: Click,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let isdirty = dirty(&document);
    let filename = document
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("instructions.md")
        .to_owned();
    let path = document.path.display().to_string();

    let mut actions = div().flex().items_center().justify_end().gap(px(8.0));
    if isdirty {
        actions = actions.child(
            Button::new("discarddocument", "Discard")
                .variant(Variant::Subtle)
                .color(ColorName::Red)
                .size(Size::Xs)
                .on_click(move |event, window, cx| ondiscard(event, window, cx)),
        );
    }
    actions = actions.child(
        Button::new("savedocument", "Save")
            .variant(Variant::Filled)
            .color(ColorName::Violet)
            .size(Size::Xs)
            .disabled(!isdirty)
            .left_section(Icon::new(IconName::Save).size(Size::Xs))
            .on_click(move |event, window, cx| onsave(event, window, cx)),
    );

    let mut editorarea = div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .px(px(34.0))
        .py(px(24.0))
        .child(
            div()
                .h_full()
                .w_full()
                .max_w(px(820.0))
                .mx_auto()
                .rounded(px(12.0))
                .border_1()
                .border_color(border)
                .bg(surface)
                .overflow_hidden()
                .child(buffer::element(&document.editor)),
        );
    if let Some(error) = document.error {
        editorarea = editorarea.child(
            div()
                .absolute()
                .left(px(34.0))
                .right(px(34.0))
                .bottom(px(26.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(guise::rgba(187, 53, 69, 0.1))
                .text_color(theme.danger().hsla())
                .child(Text::new(error).size(Size::Xs)),
        );
    }

    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .min_h(px(68.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(24.0))
                .px(px(30.0))
                .border_b_1()
                .border_color(border)
                .bg(surface)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            Button::new("closedocument", "Back")
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .disabled(isdirty)
                                .left_section(Icon::new(IconName::ArrowLeft).size(Size::Xs))
                                .on_click(move |event, window, cx| onclose(event, window, cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(9.0))
                                        .child(Text::new(filename).size(Size::Sm).bold())
                                        .child(
                                            Badge::new(if isdirty { "Unsaved" } else { "Saved" })
                                                .size(Size::Sm)
                                                .color(if isdirty {
                                                    ColorName::Orange
                                                } else {
                                                    ColorName::Teal
                                                }),
                                        ),
                                )
                                .child(
                                    Text::new(format!("{} · {path}", document.tool))
                                        .size(Size::Xs)
                                        .dimmed(),
                                ),
                        ),
                )
                .child(actions),
        )
        .child(editorarea)
        .child(
            StatusBar::new()
                .height(36.0)
                .left(Text::new(buffer::label(document.format)).size(Size::Xs))
                .right(Text::new("⌘S to save").size(Size::Xs)),
        )
        .into_any_element()
}

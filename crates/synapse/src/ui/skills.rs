use crate::skill::{State, Status};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, px};
use guise::prelude::*;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One library skill and where it stands in every connected tool.
#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub description: String,
    pub files: usize,
    pub places: Vec<Status>,
}

pub struct View {
    pub rows: Vec<Row>,
    /// Skills a tool has that the library does not, worth adopting.
    pub unmanaged: Vec<(String, String)>,
    pub problems: Vec<String>,
    pub folder: String,
    pub message: Option<(String, bool)>,
}

pub struct Actions {
    pub installall: Click,
    pub refresh: Click,
    pub openfolder: Click,
    /// Install one skill everywhere, by library name.
    pub install: Box<dyn Fn(String) -> Click>,
    /// Copy a tool's own skill into the library, by (tool, skill).
    pub adopt: Box<dyn Fn(String, String) -> Click>,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        installall,
        refresh,
        openfolder,
        install,
        adopt,
    } = actions;
    let empty = view.rows.is_empty();

    div()
        .id("skillsmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(980.0))
                .mx_auto()
                .px(px(34.0))
                .py(px(28.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(
                    div()
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
                                .child(Title::new("Skills").order(2))
                                .child(
                                    Text::new(
                                        "One library, installed into every tool that reads the Agent Skills format. Edit a skill once here instead of keeping copies in step by hand.",
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
                                .gap(px(8.0))
                                .child(
                                    Button::new("skillsrefresh", "Refresh")
                                        .variant(Variant::Subtle)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .left_section(
                                            Icon::new(IconName::RefreshCw).size(Size::Xs),
                                        )
                                        .on_click(move |event, window, cx| {
                                            refresh(event, window, cx)
                                        }),
                                )
                                .child(
                                    Button::new("skillsinstallall", "Install all")
                                        .variant(Variant::Filled)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .disabled(empty)
                                        .left_section(
                                            Icon::new(IconName::Download).size(Size::Xs),
                                        )
                                        .on_click(move |event, window, cx| {
                                            installall(event, window, cx)
                                        }),
                                ),
                        ),
                )
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
                }))
                .child(if empty {
                    blank(border, surface)
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .children(
                            view.rows
                                .into_iter()
                                .enumerate()
                                .map(|(index, row)| skill(index, row, &install, border, surface)),
                        )
                        .into_any_element()
                })
                .when(!view.unmanaged.is_empty(), |element| {
                    element.child(unmanaged(view.unmanaged, &adopt, border, surface))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Text::new(view.folder).size(Size::Xs).dimmed())
                        .child(
                            Button::new("skillsfolder", "Open the library")
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| openfolder(event, window, cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn blank(border: gpui::Hsla, surface: gpui::Hsla) -> AnyElement {
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(22.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(Text::new("The library is empty").size(Size::Sm).bold())
        .child(
            Text::new(
                "Run `synapse skill create <name>` to start one, or `synapse skill adopt <name>` to bring in a skill a tool already has.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

fn skill(
    index: usize,
    row: Row,
    install: &dyn Fn(String) -> Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    let action = install(row.name.clone());
    // A skill nobody has, or whose copies have fallen behind, is the one worth
    // acting on; the button says which.
    let pending = row
        .places
        .iter()
        .filter(|place| matches!(place.state, State::Missing | State::Stale))
        .count();
    let label = match pending {
        0 => "Reinstall".to_owned(),
        _ => format!("Install ({pending})"),
    };
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
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
                                .when(row.files > 1, |element| {
                                    element.child(
                                        Badge::new(format!("{} files", row.files))
                                            .size(Size::Sm)
                                            .color(ColorName::Gray),
                                    )
                                }),
                        )
                        .child(Text::new(row.description).size(Size::Xs).dimmed()),
                )
                .child(
                    div().flex_none().child(
                        Button::new(("skillinstall", index), label)
                            .variant(Variant::Light)
                            .color(ColorName::Violet)
                            .size(Size::Xs)
                            .on_click(move |event, window, cx| action(event, window, cx)),
                    ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .children(row.places.into_iter().map(place)),
        )
        .into_any_element()
}

fn place(status: Status) -> AnyElement {
    let color = match status.state {
        State::Installed => ColorName::Teal,
        State::Stale => ColorName::Orange,
        State::Modified | State::Foreign => ColorName::Red,
        State::Missing => ColorName::Gray,
    };
    Badge::new(format!("{} · {}", status.tool, status.state.label()))
        .size(Size::Sm)
        .color(color)
        .into_any_element()
}

fn unmanaged(
    found: Vec<(String, String)>,
    adopt: &dyn Fn(String, String) -> Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
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
                    Text::new(
                        "Skills Synapse did not install. Adopt one to copy it into the library and keep it in step everywhere.",
                    )
                    .size(Size::Xs)
                    .dimmed(),
                ),
        )
        .children(found.into_iter().enumerate().map(|(index, (tool, name))| {
            let action = adopt(tool.clone(), name.clone());
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
                        .child(Text::new(name.clone()).size(Size::Xs))
                        .child(Badge::new(tool.clone()).size(Size::Sm).color(ColorName::Gray)),
                )
                .child(
                    Button::new(("skilladopt", index), "Adopt")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| action(event, window, cx)),
                )
                .into_any_element()
        }))
        .into_any_element()
}

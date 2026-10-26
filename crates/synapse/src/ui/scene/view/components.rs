use super::*;
use gpui::{AnyElement, div, px};

pub(super) fn label(state: State) -> &'static str {
    match state {
        State::Connected => "Connected",
        State::Working => "Working",
        State::Waiting => "Waiting",
        State::Blocked => "Needs you",
        State::Done => "Finished",
        State::Disconnected => "Disconnected",
    }
}

pub(super) fn selector(plugins: &[Plugin], selected: usize, cx: &mut Context<Scene>) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .children(plugins.iter().enumerate().map(|(index, plugin)| {
            Button::new(
                gpui::ElementId::Name(format!("sceneplugin{index}").into()),
                plugin.manifest.name.clone(),
            )
            .variant(if index == selected {
                Variant::Filled
            } else {
                Variant::Subtle
            })
            .size(Size::Sm)
            .on_click(cx.listener(move |this, _, _, cx| this.choose(index, cx)))
        }))
        .into_any_element()
}

pub(super) fn details(selected: Option<Session>, time: i64, cx: &mut Context<Scene>) -> AnyElement {
    let theme = guise::theme(cx);
    let mut details = div()
        .w(px(238.0))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .pl(px(18.0))
        .border_l_1()
        .border_color(theme.border().hsla());
    if let Some(session) = selected {
        details = details
            .child(Title::new(session.label().to_owned()).order(3))
            .child(detail("Harness", session.tool.clone()))
            .child(detail("Project", session.project.clone()))
            .child(detail("State", label(session.state(time)).into()))
            .child(detail(
                "Reported task",
                if session.note.is_empty() {
                    "No task reported".into()
                } else {
                    session.note.clone()
                },
            ))
            .child(detail(
                "Last seen",
                format!("{} seconds ago", (time - session.seen).max(0)),
            ))
            .child(
                Text::new(format!("Character {} of 32", session.appearance + 1))
                    .size(Size::Xs)
                    .dimmed(),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        Button::new("sceneprevious", "Previous")
                            .size(Size::Sm)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.changeappearance(-1, cx))),
                    )
                    .child(
                        Button::new("scenenext", "Next")
                            .size(Size::Sm)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.changeappearance(1, cx))),
                    ),
            );
    } else {
        details = details
            .child(Title::new("In the workshop").order(3))
            .child(
                Text::new("Select a room to see its session and choose a character.")
                    .size(Size::Sm)
                    .dimmed(),
            )
            .child(
                Text::new("Rooms and appearances are reused when the same harness reconnects in the same project.")
                    .size(Size::Xs)
                    .dimmed(),
            );
    }
    details.into_any_element()
}

pub(super) fn filters(
    projects: Vec<String>,
    selected: Option<&String>,
    cx: &mut Context<Scene>,
) -> AnyElement {
    let mut filters = div().flex().flex_wrap().gap(px(6.0)).child(
        Button::new("sceneall", "All projects")
            .size(Size::Xs)
            .variant(if selected.is_none() {
                Variant::Filled
            } else {
                Variant::Subtle
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.project = None;
                this.selected = None;
                cx.notify();
            })),
    );
    for (index, project) in projects.into_iter().enumerate() {
        let chosen = selected == Some(&project);
        filters = filters.child(
            Button::new(
                gpui::ElementId::Name(format!("sceneproject{index}").into()),
                projectname(&project),
            )
            .size(Size::Xs)
            .variant(if chosen {
                Variant::Filled
            } else {
                Variant::Subtle
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.project = Some(project.clone());
                this.selected = None;
                cx.notify();
            })),
        );
    }
    filters.into_any_element()
}

fn detail(label: &'static str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(Text::new(label).size(Size::Xs).dimmed())
        .child(Text::new(value).size(Size::Sm))
        .into_any_element()
}

pub(super) fn projectname(project: &str) -> String {
    std::path::Path::new(project)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("No project")
        .to_owned()
}

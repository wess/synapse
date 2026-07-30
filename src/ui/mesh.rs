use crate::relay::{AgentView, Message, MessageKind, WorkerView};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, px};
use guise::prelude::*;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub enabled: bool,
    pub agents: Vec<AgentView>,
    pub workers: Vec<WorkerView>,
    pub feed: Vec<Message>,
    pub error: Option<String>,
}

pub struct Actions {
    pub enable: Click,
    pub refresh: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions { enable, refresh } = actions;

    div()
        .id("meshmain")
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
                                .flex()
                                .flex_col()
                                .gap(px(7.0))
                                .child(Title::new("Agent mesh").order(2))
                                .child(
                                    Text::new(
                                        "Connected tools that have joined, the work they are reporting, and the messages between them.",
                                    )
                                    .size(Size::Sm)
                                    .dimmed(),
                                ),
                        )
                        .child(
                            Button::new("meshrefresh", "Refresh")
                                .variant(Variant::Subtle)
                                .color(ColorName::Violet)
                                .size(Size::Sm)
                                .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                                .on_click(move |event, window, cx| refresh(event, window, cx)),
                        ),
                )
                .when_some(view.error, |element, error| {
                    element.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(theme.danger().hsla())
                            .child(Icon::new(IconName::CircleAlert).size(Size::Sm))
                            .child(Text::new(error).size(Size::Sm)),
                    )
                })
                .child(if view.enabled {
                    connected(view.agents, view.workers, view.feed, border, surface, cx)
                } else {
                    off(enable, border, surface)
                }),
        )
        .into_any_element()
}

/// What the page says before anyone has switched the mesh on. The tools cost
/// context in every session that loads them, so this explains the trade rather
/// than presenting an empty table.
fn off(enable: Click, border: gpui::Hsla, surface: gpui::Hsla) -> AnyElement {
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(22.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Icon::new(IconName::Waypoints).size(Size::Sm))
                .child(Text::new("The mesh is off").size(Size::Sm).bold()),
        )
        .child(
            Text::new(
                "Turning it on adds the coordination tools to every connected tool: agents can message each other, hand work back and forth, and wait for free between tasks. They cost context in each session, so the mesh stays off until you want it.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .child(
            Text::new("Tools already running pick this up the next time they start.")
                .size(Size::Xs)
                .dimmed(),
        )
        .child(
            Button::new("meshenable", "Turn on the mesh")
                .variant(Variant::Filled)
                .color(ColorName::Violet)
                .size(Size::Sm)
                .left_section(Icon::new(IconName::Power).size(Size::Xs))
                .on_click(move |event, window, cx| enable(event, window, cx)),
        )
        .into_any_element()
}

fn connected(
    agents: Vec<AgentView>,
    workers: Vec<WorkerView>,
    feed: Vec<Message>,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(panel(
            "Agents",
            IconName::Users,
            agents.len(),
            border,
            surface,
            if agents.is_empty() {
                empty("No agent has joined yet. An agent joins when it calls register.")
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .children(
                        agents
                            .into_iter()
                            .map(|agent| agentrow(agent, theme.clone())),
                    )
                    .into_any_element()
            },
        ))
        .child(panel(
            "Workers",
            IconName::Bot,
            workers.len(),
            border,
            surface,
            if workers.is_empty() {
                empty("No background worker is running.")
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .children(workers.into_iter().map(workerrow))
                    .into_any_element()
            },
        ))
        .child(panel(
            "Feed",
            IconName::MessagesSquare,
            feed.len(),
            border,
            surface,
            if feed.is_empty() {
                empty("Nothing has been sent across the mesh yet.")
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .children(feed.into_iter().rev().map(messagerow))
                    .into_any_element()
            },
        ))
        .into_any_element()
}

fn panel(
    title: &'static str,
    icon: IconName,
    count: usize,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    body: AnyElement,
) -> AnyElement {
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(13.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Icon::new(icon).size(Size::Sm))
                        .child(Text::new(title).size(Size::Sm).bold()),
                )
                .child(Badge::new(count.to_string()).color(ColorName::Violet)),
        )
        .child(body)
        .into_any_element()
}

fn agentrow(agent: AgentView, theme: guise::Theme) -> AnyElement {
    let (label, color) = if !agent.registered {
        ("Expected", ColorName::Gray)
    } else if agent.online {
        ("Online", ColorName::Teal)
    } else {
        ("Offline", ColorName::Gray)
    };
    let detail = [
        agent.role.as_str(),
        agent.tool.as_str(),
        agent.project.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Text::new(agent.name)
                                .size(Size::Sm)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(Badge::new(label).size(Size::Sm).color(color)),
                )
                .when(!detail.is_empty(), |element| {
                    element.child(Text::new(detail).size(Size::Xs).dimmed())
                }),
        )
        .when(!agent.status.is_empty(), |element| {
            element.child(
                div().text_color(theme.text().hsla()).child(
                    Badge::new(agent.status)
                        .size(Size::Sm)
                        .color(ColorName::Blue),
                ),
            )
        })
        .into_any_element()
}

fn workerrow(worker: WorkerView) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    Text::new(worker.name)
                        .size(Size::Sm)
                        .weight(FontWeight::SEMIBOLD),
                )
                .child(Text::new(worker.log).size(Size::Xs).dimmed()),
        )
        .child(
            Badge::new(worker.status)
                .size(Size::Sm)
                .color(ColorName::Blue),
        )
        .into_any_element()
}

fn messagerow(message: Message) -> AnyElement {
    let to = match (message.kind, message.target.as_deref()) {
        (MessageKind::Direct, Some(target)) => format!("→ {target}"),
        (MessageKind::Channel, Some(target)) => format!("→ #{target}"),
        _ => "→ everyone".to_owned(),
    };
    div()
        .flex()
        .items_baseline()
        .gap(px(10.0))
        .child(
            div().flex_none().min_w(px(190.0)).child(
                Text::new(format!("{} {to}", message.sender))
                    .size(Size::Xs)
                    .dimmed(),
            ),
        )
        .child(Text::new(oneline(&message.body)).size(Size::Xs))
        .into_any_element()
}

/// Feed rows are one line each, so a multi-line message is folded rather than
/// pushing every later row off the panel.
fn oneline(body: &str) -> String {
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    match folded.char_indices().nth(160) {
        Some((at, _)) => format!("{}…", &folded[..at]),
        None => folded,
    }
}

fn empty(message: &'static str) -> AnyElement {
    Text::new(message)
        .size(Size::Xs)
        .dimmed()
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multiline_message_is_folded_into_one_row() {
        let folded = oneline("first line\n\n  second   line ");
        assert_eq!(folded, "first line second line");
        assert!(!folded.contains('\n'));
    }

    #[test]
    fn a_long_message_is_trimmed_rather_than_pushing_the_panel_open() {
        let long = oneline(&"word ".repeat(100));
        assert!(long.ends_with('…'));
        assert!(long.chars().count() <= 161);
    }
}

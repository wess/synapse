use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, px};
use guise::prelude::*;
use std::collections::{HashMap, HashSet};
use synapsecore::relay::{AgentView, Message, MessageKind, WorkerView};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub enabled: bool,
    pub showoffline: bool,
    pub agents: Vec<AgentView>,
    pub workers: Vec<WorkerView>,
    pub feed: Vec<Message>,
    pub error: Option<String>,
}

pub struct Actions {
    pub enable: Click,
    pub refresh: Click,
    pub toggleoffline: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        enable,
        refresh,
        toggleoffline,
    } = actions;

    div()
        .id("meshmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(1180.0))
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
                                .child(Title::new("Mesh").order(2))
                                .child(
                                    Text::new(
                                        "Workers, connected agents, and recent coordination activity.",
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
                    connected(
                        view.agents,
                        view.workers,
                        view.feed,
                        view.showoffline,
                        toggleoffline,
                        border,
                        surface,
                        cx,
                    )
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
        .rounded(px(6.0))
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
    mut agents: Vec<AgentView>,
    mut workers: Vec<WorkerView>,
    feed: Vec<Message>,
    showoffline: bool,
    toggleoffline: Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let workeragents = agents
        .iter()
        .cloned()
        .map(|agent| (agent.name.clone(), agent))
        .collect::<HashMap<_, _>>();
    let workernames = workers
        .iter()
        .map(|worker| worker.name.clone())
        .collect::<HashSet<_>>();
    workers.sort_by(|left, right| {
        workerrank(left, workeragents.get(&left.name))
            .cmp(&workerrank(right, workeragents.get(&right.name)))
            .then_with(|| left.name.cmp(&right.name))
    });
    agents.retain(|agent| !workernames.contains(&agent.name));
    agents.sort_by(|left, right| {
        agentrank(left)
            .cmp(&agentrank(right))
            .then_with(|| left.name.cmp(&right.name))
    });

    let (working, notworking): (Vec<_>, Vec<_>) = workers
        .into_iter()
        .partition(|worker| workerworking(workeragents.get(&worker.name)));
    let (available, unavailable): (Vec<_>, Vec<_>) =
        agents.into_iter().partition(|agent| agent.online);
    let workercount = working.len() + notworking.len();
    let onlinecount = available.len();
    let offlinecount = unavailable.len();
    let attentioncount = notworking.len();

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(div().flex().items_center().gap(px(22.0)).children([
            summaryitem(
                workercount,
                if workercount == 1 {
                    "worker"
                } else {
                    "workers"
                },
                theme.primary().hsla(),
            ),
            summaryitem(working.len(), "working", theme.success().hsla()),
            summaryitem(
                attentioncount,
                "need attention",
                if attentioncount == 0 {
                    theme.dimmed().hsla()
                } else {
                    theme.warning().hsla()
                },
            ),
            summaryitem(onlinecount, "other agents online", theme.success().hsla()),
        ]))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(16.0))
                .child(workerpanel(
                    working,
                    notworking,
                    &workeragents,
                    border,
                    surface,
                ))
                .child(rosterpanel(
                    available,
                    unavailable,
                    showoffline,
                    toggleoffline,
                    offlinecount,
                    border,
                    surface,
                    theme.clone(),
                )),
        )
        .child(feedpanel(feed, border, surface))
        .into_any_element()
}

fn summaryitem(count: usize, label: &'static str, color: gpui::Hsla) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(7.0))
        .child(div().size(px(7.0)).rounded_full().bg(color))
        .child(
            Text::new(format!("{count} {label}"))
                .size(Size::Xs)
                .weight(FontWeight::SEMIBOLD),
        )
        .into_any_element()
}

fn panelheader(title: &'static str, count: usize, icon: IconName) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Icon::new(icon).size(Size::Sm))
                .child(Text::new(title).size(Size::Sm).bold()),
        )
        .child(
            Badge::new(count.to_string())
                .size(Size::Sm)
                .color(ColorName::Gray),
        )
        .into_any_element()
}

fn workerpanel(
    working: Vec<WorkerView>,
    notworking: Vec<WorkerView>,
    agents: &HashMap<String, AgentView>,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    let count = working.len() + notworking.len();
    div()
        .flex_1()
        .min_w(px(0.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .flex()
        .flex_col()
        .child(
            div()
                .p(px(16.0))
                .child(panelheader("Workers", count, IconName::Bot)),
        )
        .child(
            div()
                .border_t_1()
                .border_color(border)
                .px(px(16.0))
                .py(px(8.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .child(Text::new("Name and activity").size(Size::Xs).dimmed()),
                )
                .child(
                    div()
                        .w(px(96.0))
                        .child(Text::new("State").size(Size::Xs).dimmed()),
                ),
        )
        .when(count == 0, |element| {
            element.child(div().p(px(16.0)).child(empty("No supervised workers.")))
        })
        .when(!working.is_empty(), |element| {
            element.child(workergroup(
                "Working",
                working,
                ColorName::Teal,
                agents,
                border,
            ))
        })
        .when(!notworking.is_empty(), |element| {
            element.child(workergroup(
                "Needs attention",
                notworking,
                ColorName::Orange,
                agents,
                border,
            ))
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn rosterpanel(
    available: Vec<AgentView>,
    unavailable: Vec<AgentView>,
    expanded: bool,
    toggle: Click,
    offlinecount: usize,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    theme: guise::Theme,
) -> AnyElement {
    let onlinecount = available.len();
    div()
        .w(px(320.0))
        .flex_none()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .flex()
        .flex_col()
        .child(
            div()
                .p(px(16.0))
                .child(panelheader("Agents", onlinecount, IconName::Users)),
        )
        .child(
            div()
                .border_t_1()
                .border_color(border)
                .px(px(16.0))
                .py(px(8.0))
                .child(Text::new("Online now").size(Size::Xs).dimmed()),
        )
        .when(onlinecount == 0, |element| {
            element.child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(empty("No other agents online.")),
            )
        })
        .when(!available.is_empty(), |element| {
            element.child(
                div().px(px(16.0)).children(
                    available
                        .into_iter()
                        .map(|agent| agentrow(agent, theme.clone(), border)),
                ),
            )
        })
        .when(offlinecount > 0, |element| {
            element.child(offlinegroup(unavailable, expanded, toggle, theme, border))
        })
        .into_any_element()
}

fn feedpanel(feed: Vec<Message>, border: gpui::Hsla, surface: gpui::Hsla) -> AnyElement {
    let count = feed.len();
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .flex()
        .flex_col()
        .child(div().p(px(16.0)).child(panelheader(
            "Recent activity",
            count,
            IconName::MessagesSquare,
        )))
        .child(
            div()
                .border_t_1()
                .border_color(border)
                .p(px(16.0))
                .when(count == 0, |element| {
                    element.child(empty("Nothing has been sent across the mesh yet."))
                })
                .when(count > 0, |element| {
                    element.children(feed.into_iter().rev().take(8).map(messagerow))
                }),
        )
        .into_any_element()
}

fn workergroup(
    label: &'static str,
    workers: Vec<WorkerView>,
    color: ColorName,
    agents: &HashMap<String, AgentView>,
    border: gpui::Hsla,
) -> AnyElement {
    let count = workers.len();
    div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(border)
        .child(
            div()
                .px(px(16.0))
                .pt(px(10.0))
                .child(groupheading(label, count, color)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .px(px(16.0))
                .children(workers.into_iter().map(|worker| {
                    let agent = agents.get(&worker.name);
                    workerrow(worker, agent, border)
                })),
        )
        .into_any_element()
}

fn offlinegroup(
    agents: Vec<AgentView>,
    expanded: bool,
    toggle: Click,
    theme: guise::Theme,
    border: gpui::Hsla,
) -> AnyElement {
    let count = agents.len();
    div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(border)
        .p(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    Text::new(format!("{count} offline or expected"))
                        .size(Size::Xs)
                        .dimmed(),
                )
                .child(
                    Button::new("meshofflinetoggle", if expanded { "Hide" } else { "Show" })
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .on_click(move |event, window, cx| toggle(event, window, cx)),
                ),
        )
        .when(expanded, |element| {
            element.child(
                div().flex().flex_col().children(
                    agents
                        .into_iter()
                        .map(|agent| agentrow(agent, theme.clone(), border)),
                ),
            )
        })
        .when(!expanded, |element| element)
        .into_any_element()
}

fn groupheading(label: &'static str, count: usize, color: ColorName) -> AnyElement {
    div()
        .pb(px(5.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .child(Text::new(label).size(Size::Xs).weight(FontWeight::SEMIBOLD))
        .child(Badge::new(count.to_string()).size(Size::Sm).color(color))
        .into_any_element()
}

fn agentrow(agent: AgentView, theme: guise::Theme, border: gpui::Hsla) -> AnyElement {
    // A person on the roster reads differently from an agent: they are not
    // something the mesh is waiting on, and nothing should be delegated to them.
    let (label, color) = if agent.human {
        ("You", ColorName::Blue)
    } else if !agent.registered {
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
    let statuscolor = statuscolor(&agent.status);
    div()
        .border_t_1()
        .border_color(border)
        .py(px(9.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
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
                })
                // What it is doing gets its own line rather than joining the
                // detail run: it is the one part of a row that changes while you
                // are watching, and it is a sentence, not a label.
                .when(!agent.note.is_empty(), |element| {
                    element.child(Text::new(agent.note.clone()).size(Size::Xs).dimmed())
                }),
        )
        // A reported status becomes stale with the connection. Showing an
        // offline row as "working" is the exact ambiguity this view must avoid.
        .when(agent.online && !agent.status.is_empty(), |element| {
            element.child(
                div()
                    .text_color(theme.text().hsla())
                    .child(Badge::new(agent.status).size(Size::Sm).color(statuscolor)),
            )
        })
        .into_any_element()
}

fn workerrow(worker: WorkerView, agent: Option<&AgentView>, border: gpui::Hsla) -> AnyElement {
    let (status, color) = workerstatus(&worker, agent);
    let detail = [
        (!worker.role.is_empty()).then_some(worker.role.clone()),
        (worker.process > 0).then(|| format!("pid {}", worker.process)),
        (worker.restarts > 0).then(|| {
            format!(
                "{} {}",
                worker.restarts,
                if worker.restarts == 1 {
                    "restart"
                } else {
                    "restarts"
                },
            )
        }),
        worker.keepalive.then_some("kept alive".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let note = agent
        .filter(|agent| agent.online)
        .map(|agent| agent.note.as_str())
        .filter(|note| !note.is_empty())
        .unwrap_or(worker.directory.as_str());

    div()
        .border_t_1()
        .border_color(border)
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(10.0))
                        .child(
                            Text::new(worker.name)
                                .size(Size::Sm)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .when(!detail.is_empty(), |element| {
                            element.child(Text::new(detail).size(Size::Xs).dimmed())
                        }),
                )
                .when(!note.is_empty(), |element| {
                    element.child(Text::new(note.to_owned()).size(Size::Xs).dimmed())
                }),
        )
        .child(
            div()
                .w(px(96.0))
                .flex_none()
                .child(Badge::new(status).size(Size::Sm).color(color)),
        )
        .into_any_element()
}

fn workerworking(agent: Option<&AgentView>) -> bool {
    agent.is_some_and(|agent| agent.online && agent.status.eq_ignore_ascii_case("working"))
}

fn workerrank(worker: &WorkerView, agent: Option<&AgentView>) -> u8 {
    match agent {
        Some(agent) if agent.online && agent.status.eq_ignore_ascii_case("working") => 0,
        Some(agent) if agent.online && agent.status.eq_ignore_ascii_case("blocked") => 1,
        Some(agent) if agent.online => 2,
        _ if worker.status == "running" => 3,
        _ => 4,
    }
}

fn agentrank(agent: &AgentView) -> u8 {
    if agent.human && agent.online {
        0
    } else if agent.online && agent.status.eq_ignore_ascii_case("working") {
        1
    } else if agent.online {
        2
    } else if agent.registered {
        3
    } else {
        4
    }
}

fn workerstatus(worker: &WorkerView, agent: Option<&AgentView>) -> (String, ColorName) {
    if let Some(agent) = agent.filter(|agent| agent.online) {
        let status = if agent.status.is_empty() {
            "Online".to_owned()
        } else {
            agent.status.clone()
        };
        return (status, statuscolor(&agent.status));
    }
    if worker.status == "running" {
        return ("Unresponsive".to_owned(), ColorName::Orange);
    }
    if worker.status == "starting" {
        return ("Starting".to_owned(), ColorName::Blue);
    }
    if worker.status.starts_with("exited") {
        let color = if worker.status == "exited (0)" {
            ColorName::Gray
        } else {
            ColorName::Orange
        };
        return ("Stopped".to_owned(), color);
    }
    (worker.status.clone(), ColorName::Gray)
}

fn statuscolor(status: &str) -> ColorName {
    match status.to_ascii_lowercase().as_str() {
        "working" => ColorName::Teal,
        "blocked" => ColorName::Orange,
        "done" | "idle" => ColorName::Gray,
        _ => ColorName::Blue,
    }
}

fn messagerow(message: Message) -> AnyElement {
    let to = match (message.kind, message.target.as_deref()) {
        (MessageKind::Direct, Some(target)) => format!("→ {target}"),
        (MessageKind::Channel, Some(target)) => format!("→ #{target}"),
        _ => "→ everyone".to_owned(),
    };
    div()
        .py(px(5.0))
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

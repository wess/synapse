use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Hsla, Window, div, px};
use guise::prelude::*;
use synapsecore::backend::{Capability, Event, Route, Run, Summary, Trigger};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub summary: Summary,
    pub events: Vec<Event>,
    pub runs: Vec<Run>,
    pub triggers: Vec<Trigger>,
    pub routes: Vec<Route>,
    pub capabilities: Vec<Capability>,
    pub paused: bool,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct Actions {
    pub refresh: Click,
    pub toggle: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions { refresh, toggle } = actions;
    let View {
        summary,
        events,
        runs,
        triggers,
        routes,
        capabilities,
        paused,
        loading,
        error,
    } = view;

    div()
        .id("logsmain")
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
                .gap(px(18.0))
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
                                .child(Title::new("Logs").order(3))
                                .child(
                                    Text::new("Live agent activity, runs, and local automation.")
                                        .size(Size::Sm)
                                        .dimmed(),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    Button::new(
                                        "logstoggle",
                                        if paused { "Follow" } else { "Pause" },
                                    )
                                    .variant(Variant::Subtle)
                                    .color(ColorName::Gray)
                                    .size(Size::Sm)
                                    .left_section(
                                        Icon::new(if paused {
                                            IconName::Play
                                        } else {
                                            IconName::Pause
                                        })
                                        .size(Size::Xs),
                                    )
                                    .on_click(move |event, window, cx| toggle(event, window, cx)),
                                )
                                .child(
                                    Button::new(
                                        "logsrefresh",
                                        if loading { "Loading…" } else { "Refresh" },
                                    )
                                    .variant(Variant::Subtle)
                                    .color(ColorName::Violet)
                                    .size(Size::Sm)
                                    .disabled(loading)
                                    .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                                    .on_click(move |event, window, cx| refresh(event, window, cx)),
                                ),
                        ),
                )
                .when_some(error, |element, error| {
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
                .child(div().flex().gap(px(10.0)).children([
                    metric(
                        "Agents",
                        summary.liveagents,
                        theme.success().hsla(),
                        border,
                        surface,
                    ),
                    metric(
                        "Running",
                        summary.running,
                        theme.primary().hsla(),
                        border,
                        surface,
                    ),
                    metric(
                        "Queued",
                        summary.queued,
                        theme.warning().hsla(),
                        border,
                        surface,
                    ),
                    metric(
                        "Errors · 24h",
                        summary.errors,
                        if summary.errors == 0 {
                            theme.dimmed().hsla()
                        } else {
                            theme.danger().hsla()
                        },
                        border,
                        surface,
                    ),
                ]))
                .child(activity(events, border, surface, cx))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(12.0))
                        .child(runs_panel(runs, border, surface))
                        .child(automation_panel(
                            triggers,
                            routes,
                            capabilities,
                            border,
                            surface,
                        )),
                ),
        )
        .into_any_element()
}

fn metric(
    label: &'static str,
    count: usize,
    color: Hsla,
    border: Hsla,
    surface: Hsla,
) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(13.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(div().size(px(7.0)).rounded_full().bg(color))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(Text::new(count.to_string()).size(Size::Lg).bold())
                .child(Text::new(label).size(Size::Xs).dimmed()),
        )
        .into_any_element()
}

fn activity(events: Vec<Event>, border: Hsla, surface: Hsla, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let rows = events.into_iter().rev().take(120).map(|event| {
        let color = match event.level {
            synapsecore::backend::Level::Error => theme.danger().hsla(),
            synapsecore::backend::Level::Warn => theme.warning().hsla(),
            synapsecore::backend::Level::Debug | synapsecore::backend::Level::Trace => {
                theme.dimmed().hsla()
            }
            synapsecore::backend::Level::Info => theme.primary().hsla(),
        };
        let source = if event.agent.is_empty() {
            event.source
        } else {
            event.agent
        };
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(border)
            .child(div().mt(px(6.0)).size(px(6.0)).rounded_full().bg(color))
            .child(Text::new(clock(event.created)).size(Size::Xs).dimmed())
            .child(
                Text::new(source)
                    .size(Size::Xs)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(Text::new(event.kind).size(Size::Xs).dimmed())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Text::new(event.message).size(Size::Xs)),
            )
    });
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(panel_header("Activity", "newest first", IconName::Activity))
        .when(rows.len() == 0, |element| {
            element.child(empty("No activity has been recorded yet."))
        })
        .children(rows)
        .into_any_element()
}

fn runs_panel(runs: Vec<Run>, border: Hsla, surface: Hsla) -> AnyElement {
    let count = runs.len();
    let rows = runs.into_iter().take(8).map(|run| {
        div()
            .px(px(12.0))
            .py(px(9.0))
            .border_t_1()
            .border_color(border)
            .flex()
            .items_center()
            .gap(px(9.0))
            .child(
                Badge::new(format!("{:?}", run.status).to_lowercase())
                    .size(Size::Sm)
                    .color(ColorName::Gray),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Text::new(run.harness).size(Size::Xs)),
            )
            .child(Text::new(run.project).size(Size::Xs).dimmed())
    });
    div()
        .flex_1()
        .min_w(px(0.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(panel_header(
            "Runs",
            &format!("{count} recent"),
            IconName::ListTodo,
        ))
        .when(count == 0, |element| element.child(empty("No runs yet.")))
        .children(rows)
        .into_any_element()
}

fn automation_panel(
    triggers: Vec<Trigger>,
    routes: Vec<Route>,
    capabilities: Vec<Capability>,
    border: Hsla,
    surface: Hsla,
) -> AnyElement {
    let items = [
        (
            "Triggers",
            triggers.iter().filter(|item| item.enabled).count(),
            triggers.len(),
        ),
        (
            "Model routes",
            routes.iter().filter(|item| item.enabled).count(),
            routes.len(),
        ),
        ("Capabilities", capabilities.len(), capabilities.len()),
    ];
    div()
        .w(px(330.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(panel_header(
            "Automation",
            "local policy",
            IconName::Workflow,
        ))
        .children(items.into_iter().map(|(label, active, total)| {
            div()
                .px(px(12.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_between()
                .child(Text::new(label).size(Size::Xs))
                .child(
                    Text::new(format!("{active} active · {total} total"))
                        .size(Size::Xs)
                        .dimmed(),
                )
        }))
        .into_any_element()
}

fn panel_header(title: &'static str, detail: &str, icon: IconName) -> AnyElement {
    div()
        .px(px(12.0))
        .py(px(10.0))
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
        .child(Text::new(detail.to_owned()).size(Size::Xs).dimmed())
        .into_any_element()
}

fn empty(message: &'static str) -> AnyElement {
    div()
        .px(px(12.0))
        .py(px(18.0))
        .child(Text::new(message).size(Size::Xs).dimmed())
        .into_any_element()
}

fn clock(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "--:--:--".to_owned())
}

use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Hsla, Window, div, px};
use guise::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
use synapsecore::backend::{
    Capability, Change, Checkpoint, Comment, Event, Preview, Review, Route, Run, RunStats, Runner,
    Summary, Trigger, Workspace,
};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type Inspect = Rc<dyn Fn(String, &ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Clone, Default)]
pub struct RunDetail {
    pub changes: Vec<Change>,
    pub reviews: Vec<Review>,
    pub comments: Vec<Comment>,
    pub checkpoints: Vec<Checkpoint>,
    pub previews: Vec<Preview>,
    pub workspace: Option<Workspace>,
}

pub struct View {
    pub summary: Summary,
    pub events: Vec<Event>,
    pub runs: Vec<Run>,
    pub runstats: Vec<RunStats>,
    pub rundetails: HashMap<String, RunDetail>,
    pub selectedrun: Option<String>,
    pub runners: Vec<Runner>,
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
    pub inspect: Inspect,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        refresh,
        toggle,
        inspect,
    } = actions;
    let View {
        summary,
        events,
        runs,
        runstats,
        rundetails,
        selectedrun,
        runners,
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
                                .child(Title::new("Agent activity").order(3))
                                .child(
                                    Text::new(
                                        "Follow what agents changed, review their work, and see where it can run.",
                                    )
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
                        .child(runs_panel(
                            runs,
                            runstats,
                            rundetails,
                            selectedrun,
                            inspect,
                            border,
                            surface,
                        ))
                        .child(
                            div()
                                .w(px(330.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(runners_panel(runners, border, surface))
                                .child(automation_panel(
                                    triggers,
                                    routes,
                                    capabilities,
                                    border,
                                    surface,
                                )),
                        ),
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
        .px(px(11.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(div().size(px(7.0)).rounded_full().bg(color))
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(5.0))
                .child(Text::new(count.to_string()).size(Size::Sm).bold())
                .child(Text::new(label).size(Size::Xs).dimmed()),
        )
        .into_any_element()
}

fn activity(events: Vec<Event>, border: Hsla, surface: Hsla, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let mut seen = std::collections::HashSet::new();
    let rows = events
        .into_iter()
        .rev()
        .filter(|event| {
            seen.insert((
                event.agent.clone(),
                event.source.clone(),
                event.kind.clone(),
                event.message.clone(),
            ))
        })
        .take(7)
        .map(|event| {
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
                .child(
                    div()
                        .w(px(62.0))
                        .flex_none()
                        .child(Text::new(clock(event.created)).size(Size::Xs).dimmed()),
                )
                .child(
                    div().w(px(100.0)).flex_none().child(
                        Text::new(source)
                            .size(Size::Xs)
                            .weight(FontWeight::SEMIBOLD),
                    ),
                )
                .child(
                    div()
                        .w(px(110.0))
                        .flex_none()
                        .child(Text::new(event.kind).size(Size::Xs).dimmed()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(Text::new(event.message).size(Size::Xs)),
                )
        })
        .collect::<Vec<_>>();
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(panel_header(
            "Activity",
            "newest unique events",
            IconName::Activity,
        ))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .px(px(28.0))
                .py(px(6.0))
                .border_t_1()
                .border_color(border)
                .child(
                    div()
                        .w(px(62.0))
                        .child(Text::new("Time").size(Size::Xs).dimmed()),
                )
                .child(
                    div()
                        .w(px(100.0))
                        .child(Text::new("Source").size(Size::Xs).dimmed()),
                )
                .child(
                    div()
                        .w(px(110.0))
                        .child(Text::new("Event").size(Size::Xs).dimmed()),
                )
                .child(Text::new("Message").size(Size::Xs).dimmed()),
        )
        .when(rows.is_empty(), |element| {
            element.child(empty("No activity has been recorded yet."))
        })
        .children(rows)
        .into_any_element()
}

fn runs_panel(
    runs: Vec<Run>,
    runstats: Vec<RunStats>,
    rundetails: HashMap<String, RunDetail>,
    selectedrun: Option<String>,
    inspect: Inspect,
    border: Hsla,
    surface: Hsla,
) -> AnyElement {
    let count = runs.len();
    let stats = runstats
        .into_iter()
        .map(|stats| (stats.runid.clone(), stats))
        .collect::<HashMap<_, _>>();
    let rows = runs.into_iter().take(8).enumerate().map(|(index, run)| {
        let runid = run.id.clone();
        let selected = selectedrun.as_deref() == Some(runid.as_str());
        let rundetail = rundetails.get(&runid).cloned().unwrap_or_default();
        let inspect = inspect.clone();
        let stats = stats.get(&run.id);
        let name = if run.title.is_empty() {
            run.harness.clone()
        } else {
            run.title.clone()
        };
        let detail = stats
            .map(run_detail)
            .unwrap_or_else(|| "No recorded changes".to_owned());
        div()
            .px(px(12.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(border)
            .flex()
            .items_start()
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
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(Text::new(name).size(Size::Xs).bold())
                    .child(Text::new(detail).size(Size::Xs).dimmed())
                    .when(selected, |element| element.child(review_summary(rundetail))),
            )
            .child(
                div()
                    .max_w(px(180.0))
                    .child(Text::new(run.project).size(Size::Xs).dimmed()),
            )
            .child(
                Button::new(
                    ("inspect", index),
                    if selected { "Reviewing" } else { "Review" },
                )
                .variant(if selected {
                    Variant::Filled
                } else {
                    Variant::Subtle
                })
                .color(ColorName::Violet)
                .size(Size::Xs)
                .on_click(move |event, window, cx| inspect(runid.clone(), event, window, cx)),
            )
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
        .when(count == 0, |element| {
            element.child(empty(
                "No runs yet. Connected harnesses create one when they start durable work.",
            ))
        })
        .children(rows)
        .into_any_element()
}

fn review_summary(detail: RunDetail) -> AnyElement {
    let verdict = detail
        .reviews
        .first()
        .map(|review| format!("{} · {}", review.verdict, review.summary))
        .unwrap_or_else(|| "No verdict yet".to_owned());
    let open = detail
        .comments
        .iter()
        .filter(|comment| !comment.resolved)
        .count();
    let workspace = detail
        .workspace
        .as_ref()
        .map(|workspace| format!("{} ({})", workspace.branch, workspace.status))
        .unwrap_or_else(|| "none".to_owned());
    let provenance = if detail.changes.is_empty() {
        "No file changes recorded".to_owned()
    } else {
        detail
            .changes
            .iter()
            .take(3)
            .map(|change| format!("{} {}", change.operation, change.path))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    div()
        .mt(px(5.0))
        .p(px(8.0))
        .rounded(px(6.0))
        .bg(guise::rgb(32, 31, 39))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(Text::new(format!("Verdict: {verdict}")).size(Size::Xs))
        .child(Text::new(format!("Changes: {provenance}")).size(Size::Xs))
        .child(
            Text::new(format!(
                "Notes: {open} open · Recovery: {} checkpoints · Previews: {} · Workspace: {workspace}",
                detail.checkpoints.len(),
                detail.previews.len(),
            ))
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

fn run_detail(stats: &RunStats) -> String {
    let mut parts = Vec::new();
    if stats.changes > 0 {
        parts.push(format!("{} change{}", stats.changes, plural(stats.changes)));
    }
    if stats.reviews > 0 {
        parts.push(format!("{} review{}", stats.reviews, plural(stats.reviews)));
    }
    if stats.opencomments > 0 {
        parts.push(format!(
            "{} open note{}",
            stats.opencomments,
            plural(stats.opencomments)
        ));
    }
    if stats.checkpoints > 0 {
        parts.push(format!(
            "{} checkpoint{}",
            stats.checkpoints,
            plural(stats.checkpoints)
        ));
    }
    if stats.previews > 0 {
        parts.push(format!(
            "{} live preview{}",
            stats.previews,
            plural(stats.previews)
        ));
    }
    if !stats.workspace.is_empty() {
        parts.push(format!("workspace {}", stats.workspace));
    }
    if parts.is_empty() {
        "No recorded changes".to_owned()
    } else {
        parts.join(" · ")
    }
}

fn plural(count: i64) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn runners_panel(runners: Vec<Runner>, border: Hsla, surface: Hsla) -> AnyElement {
    let count = runners.len();
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(panel_header("Runners", &format!("{count} known"), IconName::Monitor))
        .when(count == 0, |element| {
            element.child(empty(
                "No runners registered. A runner gives durable jobs a machine and a capability list.",
            ))
        })
        .children(runners.into_iter().take(6).map(|runner| {
            div()
                .px(px(12.0))
                .py(px(9.0))
                .border_t_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(Text::new(runner.name).size(Size::Xs).bold())
                        .child(
                            Text::new(if runner.local { "this machine" } else { "remote" })
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                )
                .child(Badge::new(runner.status).size(Size::Sm).color(ColorName::Gray))
        }))
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
            let local = time.with_timezone(&chrono::Local);
            let format = if local.date_naive() == chrono::Local::now().date_naive() {
                "%H:%M:%S"
            } else {
                "%b %-d %H:%M"
            };
            local.format(format).to_string()
        })
        .unwrap_or_else(|| "--:--:--".to_owned())
}

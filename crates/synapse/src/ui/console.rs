//! The console — a place to talk to the agents rather than watch them.
//!
//! Three columns, borrowed from nora's deck: what has been said, what the mesh
//! is doing, and who is on it. The composer is the point; the Mesh page next
//! door already reports, and reporting is not the same as being able to answer.
//!
//! Two things it deliberately does not do. It does not put itself between you
//! and the agents — you are a row on the roster like everyone else, and every
//! worker stays directly addressable, which is the whole argument `synapse mux`
//! makes for existing. And it does not invent activity: a still column means a
//! quiet mesh, so nothing here animates on a timer or fills a gap with a
//! placeholder.

use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Entity, FontWeight, Window, div, px};
use guise::TextInput;
use guise::prelude::*;
use synapsecore::relay::{AgentView, Message, MessageKind, WorkerView};

/// How long a message rings for, in seconds.
///
/// Taken from the reactor when there is one, so the two cannot disagree about
/// how long a ring lives; a plain number when there is not, so the dashboard
/// can go on measuring without the dependency.
#[cfg(feature = "reactor")]
pub const RINGLIFE: f32 = hud::PULSE_LIFE;
#[cfg(not(feature = "reactor"))]
pub const RINGLIFE: f32 = 1.1;

/// What the mesh is doing, in the states the console can draw.
///
/// The console's own enum rather than the reactor's, so the page and the
/// dashboard that feeds it both compile with the reactor turned off.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Life {
    /// Nobody here but you.
    #[default]
    Idle,
    /// Agents registered and parked.
    Waiting,
    /// At least one of them working.
    Working,
    /// Something was said just now.
    Talking,
}

/// A frame of mesh, measured. Every field came off the roster or the feed.
#[derive(Clone, Debug, Default)]
#[cfg_attr(
    not(feature = "reactor"),
    allow(
        dead_code,
        reason = "the reactor is what reads these; the shape stays so the \
                  dashboard needs no cfg of its own"
    )
)]
pub struct Pulse {
    /// Monotonic seconds. The only value here not read from the mesh; it turns
    /// the sweep.
    pub phase: f32,
    /// Share of the agents that are working, 0..1.
    pub level: f32,
    /// One per agent: how busy that agent is, 0..1.
    pub bands: Vec<f32>,
    /// Ages in seconds of messages young enough to still be ringing.
    pub rings: Vec<f32>,
}

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    /// The name you are on the roster under, or why you are not on it.
    pub identity: Result<String, String>,
    pub focus: Option<String>,
    /// Oldest first, so the newest line is at the bottom where it was typed.
    pub feed: Vec<Message>,
    pub agents: Vec<AgentView>,
    pub workers: Vec<WorkerView>,
    /// Most workers one session may run, so the roster can say what is left.
    pub limit: usize,
    /// What the reactor draws from. Every field is something the mesh actually
    /// reported, which is what makes a still reactor mean a quiet mesh rather
    /// than a broken window.
    pub pulse: Pulse,
    pub life: Life,
    pub composer: Entity<TextInput>,
    pub message: Option<(String, bool)>,
}

pub struct Actions {
    pub send: Click,
    pub refresh: Click,
    /// Address a bare line at one agent, by name.
    pub focus: Box<dyn Fn(String) -> Click>,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        send,
        refresh,
        focus,
    } = actions;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(px(0.0))
                .gap(px(12.0))
                .p(px(14.0))
                .child(transcript(view.feed, &view.identity, border, surface))
                .child(stage(
                    &view.agents,
                    &view.workers,
                    view.limit,
                    view.focus.as_deref(),
                    view.life,
                    &view.pulse,
                    border,
                    surface,
                    theme.primary().hsla(),
                ))
                .child(roster(
                    view.agents,
                    view.workers,
                    view.focus.as_deref(),
                    &focus,
                    border,
                    surface,
                )),
        )
        .child(composer(
            view.composer,
            view.focus,
            view.identity,
            view.message,
            send,
            refresh,
            border,
            surface,
            theme.danger().hsla(),
            theme.success().hsla(),
        ))
        .into_any_element()
}

/// What has been said. A log, not a chat: sender label above the line, no
/// bubbles and no alignment games, because most of these lines are agents
/// talking to each other and only some of them are to you.
fn transcript(
    feed: Vec<Message>,
    identity: &Result<String, String>,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    let me = identity.as_deref().ok().map(str::to_owned);
    let empty = feed.is_empty();
    div()
        .id("consoletranscript")
        .w(px(340.0))
        .flex_none()
        .flex()
        .flex_col()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_y_scroll()
        .child(
            div()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(Text::new("TRANSCRIPT").size(Size::Xs).dimmed())
                .when(empty, |element| {
                    element.child(
                        Text::new(
                            "Nothing has been said yet. Type below to reach an agent — everything on the mesh shows here, including agents talking to each other.",
                        )
                        .size(Size::Xs)
                        .dimmed(),
                    )
                })
                .children(feed.into_iter().map(|message| {
                    let mine = me.as_deref() == Some(message.sender.as_str());
                    let label = match message.kind {
                        MessageKind::Direct => match message.target.as_deref() {
                            Some(to) => format!("{} → {to}", message.sender),
                            None => message.sender.clone(),
                        },
                        MessageKind::Channel => format!(
                            "{} → #{}",
                            message.sender,
                            message.target.as_deref().unwrap_or_default()
                        ),
                        MessageKind::Broadcast => format!("{} → everyone", message.sender),
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            Text::new(label.to_uppercase())
                                .size(Size::Xs)
                                .weight(if mine {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .dimmed(),
                        )
                        .child(Text::new(message.body).size(Size::Sm))
                        .into_any_element()
                })),
        )
        .into_any_element()
}

/// The middle column: what the mesh is actually doing, as a reactor and as the
/// numbers behind it.
///
/// The reactor is `nora-hud`'s, and it keeps that crate's rule — nothing
/// animates without input. Where nora hands it a frame of audio, this hands it
/// a frame of mesh: a ring per message that landed, a band per agent, a level
/// that is the share of them working. A still reactor is a quiet mesh.
#[allow(clippy::too_many_arguments, reason = "one column, assembled once")]
fn stage(
    agents: &[AgentView],
    workers: &[WorkerView],
    limit: usize,
    focus: Option<&str>,
    life: Life,
    pulse: &Pulse,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    accent: gpui::Hsla,
) -> AnyElement {
    let people = agents.iter().filter(|agent| agent.human).count();
    let online = agents.iter().filter(|agent| agent.online).count();
    let busy = agents
        .iter()
        .filter(|agent| !agent.human && agent.status == "working")
        .count();
    div()
        .id("consolestage")
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .overflow_y_scroll()
        .child(Text::new("MESH").size(Size::Xs).dimmed())
        .children(reactor(life, pulse, accent))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(fact("on the mesh", online.to_string()))
                .child(fact("working", busy.to_string()))
                .child(fact("people", people.to_string()))
                .child(fact("workers", format!("{} of {limit}", workers.len()))),
        )
        .when_some(focus, |element, name| {
            element.child(
                Text::new(format!("A line with no @ goes to {name}."))
                    .size(Size::Xs)
                    .dimmed(),
            )
        })
        .when(focus.is_none(), |element| {
            // The status bar already spells out the three prefixes, so this
            // says the one thing it does not: that nothing is aimed yet.
            element.child(
                Text::new("No agent picked — a bare line has nowhere to go yet.")
                    .size(Size::Xs)
                    .dimmed(),
            )
        })
        .into_any_element()
}

/// The reactor, when the build has one. `None` is not a gap to fill: the fact
/// strip below already says what it says, so the column simply starts there.
#[cfg(feature = "reactor")]
fn reactor(life: Life, pulse: &Pulse, accent: gpui::Hsla) -> Option<AnyElement> {
    let motion = hud::Motion {
        phase: pulse.phase,
        level: pulse.level,
        bands: pulse.bands.clone(),
        peaks: Vec::new(),
        pulses: pulse.rings.clone(),
    };
    let activity = match life {
        Life::Idle => hud::Activity::Idle,
        Life::Waiting => hud::Activity::Listening,
        Life::Working => hud::Activity::Thinking,
        Life::Talking => hud::Activity::Speaking,
    };
    Some(
        div()
            .flex()
            .items_center()
            .justify_center()
            .py(px(6.0))
            .child(
                hud::Reactor::new(activity, motion)
                    .size(150.0)
                    .color(accent),
            )
            .into_any_element(),
    )
}

#[cfg(not(feature = "reactor"))]
fn reactor(_life: Life, _pulse: &Pulse, _accent: gpui::Hsla) -> Option<AnyElement> {
    None
}

fn fact(label: &str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .min_w(px(84.0))
        .child(Text::new(value).size(Size::Lg))
        .child(Text::new(label.to_uppercase()).size(Size::Xs).dimmed())
        .into_any_element()
}

/// Who is here, and what each one last said it was doing. Clicking one aims the
/// composer at it; nothing about that is required, since `@name` always works.
fn roster(
    agents: Vec<AgentView>,
    workers: Vec<WorkerView>,
    focus: Option<&str>,
    aim: &dyn Fn(String) -> Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
) -> AnyElement {
    let empty = agents.is_empty();
    div()
        .id("consoleroster")
        .w(px(280.0))
        .flex_none()
        .flex()
        .flex_col()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_y_scroll()
        .child(
            div()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(Text::new("ROSTER").size(Size::Xs).dimmed())
                .when(empty, |element| {
                    element.child(
                        Text::new(
                            "Nobody is on the mesh. `synapse mux --team overseer` starts one agent and puts you beside it.",
                        )
                        .size(Size::Xs)
                        .dimmed(),
                    )
                })
                .children(agents.into_iter().enumerate().map(|(index, agent)| {
                    let aimed = focus == Some(agent.name.as_str());
                    let action = aim(agent.name.clone());
                    let colour = if agent.human {
                        ColorName::Blue
                    } else if agent.online {
                        ColorName::Teal
                    } else {
                        ColorName::Gray
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .child(
                                    Button::new(("consolefocus", index), agent.name.clone())
                                        .variant(if aimed {
                                            Variant::Light
                                        } else {
                                            Variant::Subtle
                                        })
                                        .color(ColorName::Violet)
                                        .size(Size::Xs)
                                        .on_click(move |event, window, cx| {
                                            action(event, window, cx)
                                        }),
                                )
                                .child(
                                    Badge::new(if agent.human {
                                        "you".to_owned()
                                    } else if agent.status.is_empty() {
                                        "idle".to_owned()
                                    } else {
                                        agent.status.clone()
                                    })
                                    .size(Size::Sm)
                                    .color(colour),
                                ),
                        )
                        .when(!agent.note.trim().is_empty(), |element| {
                            element.child(Text::new(agent.note.clone()).size(Size::Xs).dimmed())
                        })
                        .into_any_element()
                }))
                .when(!workers.is_empty(), |element| {
                    element
                        .child(Text::new("WORKERS").size(Size::Xs).dimmed())
                        .children(workers.into_iter().map(|worker| {
                            Text::new(format!("{} · {}", worker.name, worker.status))
                                .size(Size::Xs)
                                .dimmed()
                                .into_any_element()
                        }))
                }),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments, reason = "one row of chrome, built once")]
fn composer(
    input: Entity<TextInput>,
    focus: Option<String>,
    identity: Result<String, String>,
    message: Option<(String, bool)>,
    send: Click,
    refresh: Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    danger: gpui::Hsla,
    success: gpui::Hsla,
) -> AnyElement {
    // Nothing is typed at a mesh you are not on, and the reason is worth more
    // than a disabled box with no explanation beside it.
    let blocked = identity.as_ref().err().cloned();
    div()
        .flex_none()
        .border_t_1()
        .border_color(border)
        .bg(surface)
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .when_some(message, |element, (text, error)| {
            element.child(
                div()
                    .text_color(if error { danger } else { success })
                    .child(Text::new(text).size(Size::Xs)),
            )
        })
        .when_some(blocked.clone(), |element, reason| {
            element.child(Text::new(reason).size(Size::Xs).dimmed())
        })
        .child(
            div()
                .flex()
                .items_end()
                .gap(px(10.0))
                .child(div().flex_1().min_w(px(0.0)).child(input))
                .child(
                    Button::new("consolerefresh", "Refresh")
                        .variant(Variant::Subtle)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .on_click(move |event, window, cx| refresh(event, window, cx)),
                )
                .child(
                    Button::new(
                        "consolesend",
                        match focus {
                            Some(name) => format!("Send to {name}"),
                            None => "Send".to_owned(),
                        },
                    )
                    .variant(Variant::Filled)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .disabled(blocked.is_some())
                    .on_click(move |event, window, cx| send(event, window, cx)),
                ),
        )
        .into_any_element()
}

use crate::ui::theme::Mode;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Window, div, px};
use guise::prelude::*;
use synapsecore::agent::GuidanceState;
use synapsecore::brain::Optimization;
use synapsecore::cli::InstallStatus;
use synapsecore::shellsetup::{Integration, IntegrationState};

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub optimization: Optimization,
    pub mesh: bool,
    pub learn: bool,
    /// Most background workers one session may run at once.
    pub workers: usize,
    /// Whether the console draws its reactor, and whether this build could.
    pub reactor: bool,
    pub reactorbuilt: bool,
    /// What the microphone can do here, said in the user's terms.
    pub voice: Voice,
    pub thememode: Mode,
    pub clistatus: InstallStatus,
    pub clipath: String,
    pub shellintegration: Option<Integration>,
    pub shellerror: Option<String>,
    pub message: Option<(String, bool)>,
    pub guidance: GuidanceState,
    pub pendingguidance: bool,
}

pub struct Actions {
    pub meshon: Click,
    pub meshoff: Click,
    pub learnon: Click,
    pub learnoff: Click,
    /// Set the worker limit, by count.
    pub setworkers: Box<dyn Fn(usize) -> Click>,
    pub reactoron: Click,
    pub reactoroff: Click,
    /// Ask for the microphone, when nobody has been asked.
    pub askvoice: Click,
    pub full: Click,
    pub balanced: Click,
    pub lean: Click,
    pub system: Click,
    pub light: Click,
    pub dark: Click,
    pub install: Click,
    pub shellinstall: Click,
    pub shellremove: Click,
    pub opensoul: Click,
    pub syncguidance: Click,
    pub adoptguidance: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        meshon,
        meshoff,
        learnon,
        learnoff,
        setworkers,
        reactoron,
        reactoroff,
        askvoice,
        full,
        balanced,
        lean,
        system,
        light,
        dark,
        install,
        shellinstall,
        shellremove,
        opensoul,
        syncguidance,
        adoptguidance,
    } = actions;
    div()
        .id("settingsmain")
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
                        .flex_col()
                        .gap(px(7.0))
                        .child(Title::new("Settings").order(2))
                        .child(
                            Text::new(
                                "Manage shared guidance, recall, appearance, CLI, and shell integration.",
                            )
                            .size(Size::Sm)
                            .dimmed(),
                        )
                        .when_some(view.message, |element, (message, error)| {
                            element.child(
                                div()
                                    .mt(px(4.0))
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
                        }),
                )
                .child(
                    div()
                        .rounded(px(14.0))
                        .border_1()
                        .border_color(border)
                        .bg(surface)
                        .p(px(18.0))
                        .flex()
                        .flex_col()
                        .gap(px(14.0))
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
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(Icon::new(IconName::Gauge).size(Size::Sm))
                                                .child(
                                                    Text::new("Recall optimization")
                                                        .size(Size::Sm)
                                                        .bold(),
                                                ),
                                        )
                                        .child(
                                            Text::new(
                                                "Original memories stay untouched. Optimization applies only to MCP recall responses.",
                                            )
                                            .size(Size::Xs)
                                            .dimmed(),
                                        ),
                                )
                                .child(
                                    Badge::new(view.optimization.name())
                                        .color(ColorName::Violet),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(10.0))
                                .child(option(
                                    "optimization",
                                    "Full",
                                    "Up to 25 results · original formatting · no response budget",
                                    view.optimization == Optimization::Full,
                                    full,
                                ))
                                .child(option(
                                    "optimization",
                                    "Balanced",
                                    "Up to 8 results · compact whitespace · about 1,500 tokens",
                                    view.optimization == Optimization::Balanced,
                                    balanced,
                                ))
                                .child(option(
                                    "optimization",
                                    "Lean",
                                    "Up to 4 results · deduplicated · about 700 tokens",
                                    view.optimization == Optimization::Lean,
                                    lean,
                                )),
                        )
                        .child(
                            Text::new(
                                "Token counts are estimates. Connected tools may request a smaller per-call budget, but they cannot exceed this setting.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(meshpanel(view.mesh, meshon, meshoff, border, surface))
                .child(learnpanel(view.learn, learnon, learnoff, border, surface))
                .child(workerpanel(view.workers, &setworkers, border, surface))
                .child(reactorpanel(
                    view.reactor,
                    view.reactorbuilt,
                    reactoron,
                    reactoroff,
                    border,
                    surface,
                    theme.dimmed().hsla(),
                ))
                .child(voicepanel(view.voice, askvoice, border, surface))
                .child(guidancepanel(
                    view.guidance,
                    view.pendingguidance,
                    opensoul,
                    syncguidance,
                    adoptguidance,
                    cx,
                ))
                .child(shellmodes(
                    view.shellintegration,
                    view.shellerror,
                    shellinstall,
                    shellremove,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
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
                                        .gap(px(8.0))
                                        .child(Icon::new(IconName::Monitor).size(Size::Sm))
                                        .child(Text::new("Appearance").size(Size::Sm).bold()),
                                )
                                .child(
                                    Text::new("System follows the operating system as it changes.")
                                        .size(Size::Xs)
                                        .dimmed(),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(7.0))
                                        .child(themebutton(
                                            "System",
                                            IconName::Monitor,
                                            view.thememode == Mode::System,
                                            system,
                                        ))
                                        .child(themebutton(
                                            "Light",
                                            IconName::Sun,
                                            view.thememode == Mode::Light,
                                            light,
                                        ))
                                        .child(themebutton(
                                            "Dark",
                                            IconName::Moon,
                                            view.thememode == Mode::Dark,
                                            dark,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .rounded(px(14.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface)
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
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
                                                .child(
                                                    Icon::new(IconName::Terminal).size(Size::Sm),
                                                )
                                                .child(Text::new("Command line").size(Size::Sm).bold()),
                                        )
                                        .child(
                                            Badge::new(match view.clistatus {
                                                InstallStatus::Installed(_) => "Installed",
                                                InstallStatus::Conflict(_) => "Conflict",
                                                InstallStatus::Missing => "Not installed",
                                            })
                                            .color(match view.clistatus {
                                                InstallStatus::Installed(_) => ColorName::Teal,
                                                InstallStatus::Conflict(_) => ColorName::Orange,
                                                InstallStatus::Missing => ColorName::Gray,
                                            }),
                                        ),
                                )
                                .child(Text::new(view.clipath).size(Size::Xs).dimmed())
                                .child(
                                    Button::new("installsettingscli", "Install CLI")
                                        .variant(Variant::Light)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .disabled(matches!(
                                            view.clistatus,
                                            InstallStatus::Installed(_)
                                        ))
                                        .left_section(
                                            Icon::new(IconName::Download).size(Size::Xs),
                                        )
                                        .on_click(move |event, window, cx| {
                                            install(event, window, cx)
                                        }),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

/// The mesh switch. Its tools are loaded by every connected tool, so the panel
/// says what turning it on costs as well as what it buys.
fn meshpanel(
    enabled: bool,
    on: Click,
    off: Click,
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
        .gap(px(13.0))
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Waypoints).size(Size::Sm))
                                .child(Text::new("Agent mesh").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "Lets connected tools message each other, hand work back and forth, and wait for free between tasks.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(
                    Badge::new(if enabled { "On" } else { "Off" }).color(if enabled {
                        ColorName::Teal
                    } else {
                        ColorName::Gray
                    }),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(option(
                    "mesh",
                    "Off",
                    "Memory and vault tools only · the smallest tool list",
                    !enabled,
                    off,
                ))
                .child(option(
                    "mesh",
                    "On",
                    "Adds the coordination tools · costs context in every session",
                    enabled,
                    on,
                )),
        )
        .child(
            Text::new(
                "Messages stay in the same local database as your memory. Tools already running pick this up the next time they start.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

/// The self-improvement switch. Same shape as the mesh, and the same bargain:
/// two more tools in every session, plus agents writing into a library. What
/// makes it safe to leave on is on the Skills page, so this says so.
fn learnpanel(
    enabled: bool,
    on: Click,
    off: Click,
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
        .gap(px(13.0))
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Sparkles).size(Size::Sm))
                                .child(Text::new("Self-improvement").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "Lets a session write down a procedure it worked out as a skill, and correct one that turned out wrong.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(
                    Badge::new(if enabled { "On" } else { "Off" }).color(if enabled {
                        ColorName::Teal
                    } else {
                        ColorName::Gray
                    }),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(option(
                    "learn",
                    "Off",
                    "Only you write skills · the smallest tool list",
                    !enabled,
                    off,
                ))
                .child(option(
                    "learn",
                    "On",
                    "Agents may write and correct skills · costs context in every session",
                    enabled,
                    on,
                )),
        )
        .child(
            Text::new(
                "A skill an agent writes goes into the library and into no tool. It waits on the Skills page until you approve it, so nothing changes how your sessions behave without you having read it first.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

/// The fan-out bound. Every worker is a real session on the same account, so
/// the panel says what the number costs rather than presenting it as capacity.
fn workerpanel(
    workers: usize,
    set: &dyn Fn(usize) -> Click,
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
        .gap(px(13.0))
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Users).size(Size::Sm))
                                .child(Text::new("Background workers").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "The most agents one supervisor may run at once. Each is a separate session on the account you already pay for, so this is a spending limit as much as a performance one.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(Badge::new(workers.to_string()).color(ColorName::Teal)),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .children([2_usize, 4, 8, 16].into_iter().map(|count| {
                    let action = set(count);
                    let label = count.to_string();
                    option(
                        "workers",
                        &label,
                        match count {
                            2 => "One thing at a time, with a reviewer",
                            4 => "A small team",
                            8 => "The default",
                            _ => "As many as most machines will carry",
                        },
                        workers == count,
                        action,
                    )
                    .into_any_element()
                })),
        )
        .child(
            Text::new(
                "`synapse settings workers <count>` takes any number up to the built-in ceiling. A supervisor already running picks a change up on its next spawn.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

/// What a build can do about speech, and what the person has to do next.
///
/// Four states and four different next steps, which is the whole reason this is
/// a panel rather than a switch. "Off" and "you have not been asked yet" and
/// "you said no" and "this build cannot" all look the same from a toggle.
#[derive(Clone, Debug, PartialEq, Eq)]
// Which variants are reachable depends on the build: `Absent` only without the
// feature, the rest only with it. Neither configuration constructs all five, and
// the panel has to be able to explain any of them.
#[allow(
    dead_code,
    reason = "the reachable set differs per build configuration"
)]
pub enum Voice {
    /// Built without the feature.
    Absent,
    /// Built with it, and this Mac cannot transcribe on-device.
    Unsupported,
    Ask,
    Allowed,
    Refused,
}

fn voicepanel(voice: Voice, ask: Click, border: gpui::Hsla, surface: gpui::Hsla) -> AnyElement {
    let (badge, colour, body) = match voice {
        Voice::Absent => (
            "Not in this build",
            ColorName::Gray,
            "Dictation is off by default and is the only thing here that wants a microphone. A build without it has no Speech framework in it and is signed with no microphone entitlement, which is the right shape for a program holding a Keychain reference to every secret you own. Rebuild with `--features voice` to have it.",
        ),
        Voice::Unsupported => (
            "Unavailable",
            ColorName::Gray,
            "This Mac cannot transcribe without sending audio to Apple. Synapse will not transcribe at all rather than do that, so dictation stays off here.",
        ),
        Voice::Ask => (
            "Needs permission",
            ColorName::Orange,
            "macOS has not asked you yet. Nothing is recorded until you allow it, and the microphone opens only while you are dictating.",
        ),
        Voice::Allowed => (
            "Ready",
            ColorName::Teal,
            "The microphone button is on the console. Speech is transcribed on this Mac — no vendor, no key, and nothing billed per minute.",
        ),
        Voice::Refused => (
            "Refused",
            ColorName::Red,
            "Synapse is not allowed to use the microphone. System Settings › Privacy & Security › Microphone is where that is changed; nothing here can change it for you.",
        ),
    };
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
                .items_start()
                .justify_between()
                .gap(px(24.0))
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
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Mic).size(Size::Sm))
                                .child(Text::new("Dictation").size(Size::Sm).bold()),
                        )
                        .child(Text::new(body).size(Size::Xs).dimmed()),
                )
                .child(Badge::new(badge).color(colour)),
        )
        .when(voice == Voice::Ask, |element| {
            element.child(
                div().flex().child(
                    Button::new("askvoice", "Ask for the microphone")
                        .variant(Variant::Light)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .on_click(move |event, window, cx| ask(event, window, cx)),
                ),
            )
        })
        .into_any_element()
}

/// The reactor switch. Nothing is lost by turning it off, which the panel says
/// rather than leaving somebody to wonder what they gave up.
fn reactorpanel(
    on: bool,
    built: bool,
    enable: Click,
    disable: Click,
    border: gpui::Hsla,
    surface: gpui::Hsla,
    dimmed: gpui::Hsla,
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
                .items_start()
                .justify_between()
                .gap(px(24.0))
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
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Activity).size(Size::Sm))
                                .child(Text::new("Console reactor").size(Size::Sm).bold())
                                .child(Badge::new("Beta").size(Size::Sm).color(ColorName::Orange))
                                .child(
                                    // The badge says it is new; the hint says
                                    // what that costs you, which is the half a
                                    // label on its own never manages.
                                    div()
                                        .id("reactorbeta")
                                        .flex()
                                        .items_center()
                                        .text_color(dimmed)
                                        .child(Icon::new(IconName::CircleHelp).size(Size::Xs))
                                        .tooltip(guise::tooltip(
                                            "New, and drawn every frame while the console is open. If it costs you battery or draws wrong, turn it off — the numbers beside it say the same things.",
                                        )),
                                ),
                        )
                        .child(
                            Text::new(match built {
                                true => "The dial in the middle of the console, driven by the mesh: a ring for each message, a band for each agent. Turning it off removes it, and the numbers beside it say the same things.",
                                false => "This build has no reactor in it. Everything it would show, the numbers on the console show too. Rebuild with the `reactor` feature to have the dial.",
                            })
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(
                    Badge::new(match (built, on) {
                        (false, _) => "Not in this build",
                        (true, true) => "On",
                        (true, false) => "Off",
                    })
                    .color(match (built, on) {
                        (false, _) => ColorName::Gray,
                        (true, true) => ColorName::Teal,
                        (true, false) => ColorName::Gray,
                    }),
                ),
        )
        .when(built, |element| {
            element.child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(option("reactor", "Off", "Just the numbers", !on, disable))
                    .child(option("reactor", "On", "Draw the dial", on, enable)),
            )
        })
        .into_any_element()
}

fn guidancepanel(
    guidance: GuidanceState,
    pending: bool,
    open: Click,
    sync: Click,
    adopt: Click,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let ready = guidance.exists && guidance.synced == guidance.total;
    let label = if guidance.consolidated && ready {
        "One source"
    } else if ready {
        "Pointers ready"
    } else if guidance.stale > 0 {
        "Update needed"
    } else {
        "Setup needed"
    };
    let description = if guidance.stale > 0 {
        "These tools carry a Synapse block from an older release. Sync to update it so sessions announce whether Synapse is connected."
    } else if !ready {
        "Create SOUL.md and connect both global instruction files with managed pointers."
    } else if guidance.consolidated {
        "Both global instruction files contain only managed pointers to SOUL.md."
    } else {
        "SOUL.md owns shared guidance. Sync preserves existing global text; consolidation moves it into the shared file and keeps backups."
    };
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::BookOpenCheck).size(Size::Sm))
                                .child(Text::new("Shared guidance").size(Size::Sm).bold()),
                        )
                        .child(Text::new(description).size(Size::Xs).dimmed()),
                )
                .child(
                    Badge::new(label).color(if ready {
                        ColorName::Teal
                    } else {
                        ColorName::Orange
                    }),
                ),
        )
        .child(
            div()
                .rounded(px(10.0))
                .border_1()
                .border_color(border)
                .p(px(13.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(Text::new("SOUL.md").size(Size::Sm).bold())
                        .child(
                            Text::new(guidance.path.display().to_string())
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new("opensoul", "Open shared guidance")
                                .variant(Variant::Subtle)
                                .color(ColorName::Gray)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::FileText).size(Size::Xs))
                                .on_click(move |event, window, cx| open(event, window, cx)),
                        )
                        .child(
                            Button::new("syncguidance", "Sync pointers")
                                .variant(Variant::Light)
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                                .on_click(move |event, window, cx| sync(event, window, cx)),
                        ),
                ),
        )
        .when(!guidance.consolidated, |element| {
            element.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(18.0))
                    .child(
                        Text::new(
                            "Consolidation is optional and replaces the two global files only after confirmation.",
                        )
                        .size(Size::Xs)
                        .dimmed(),
                    )
                    .child(
                        Button::new(
                            "adoptguidance",
                            if pending {
                                "Confirm consolidation"
                            } else {
                                "Consolidate guidance"
                            },
                        )
                        .variant(if pending {
                            Variant::Filled
                        } else {
                            Variant::Subtle
                        })
                        .color(if pending {
                            ColorName::Orange
                        } else {
                            ColorName::Gray
                        })
                        .size(Size::Xs)
                        .on_click(move |event, window, cx| adopt(event, window, cx)),
                    ),
            )
        })
}

fn shellmodes(
    integration: Option<Integration>,
    error: Option<String>,
    install: Click,
    remove: Click,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let (badge, color) = match integration.as_ref().map(|item| item.state) {
        Some(IntegrationState::Installed) => ("Enabled", ColorName::Teal),
        Some(IntegrationState::Modified) => ("Needs repair", ColorName::Orange),
        Some(IntegrationState::Missing) => ("Not enabled", ColorName::Gray),
        None => ("Unavailable", ColorName::Orange),
    };
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Terminal).size(Size::Sm))
                                .child(Text::new("Shell environments").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "Choose a one-command boundary or opt into automatic activation for approved directories.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(Badge::new(badge).color(color)),
        )
        .child(shellcontrol(integration, error, install, remove, border))
        .child(
            div()
                .flex()
                .gap(px(24.0))
                .child(shellmode(
                    "Command scoped",
                    "synapse run -- cargo test",
                    "Reads Keychain values for one child process. Your current shell remains unchanged.",
                    border,
                ))
                .child(shellmode(
                    "Ambient directory",
                    "synapse allow",
                    "Once enabled above, loads an approved scope on entry, unloads it on exit, and restores previous shell values.",
                    border,
                )),
        )
        .child(
            Text::new(
                "Ambient values are available to every process launched from that shell. Leave the directory or run synapse deny to unload them.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
}

fn shellcontrol(
    integration: Option<Integration>,
    error: Option<String>,
    install: Click,
    remove: Click,
    border: gpui::Hsla,
) -> AnyElement {
    let Some(integration) = integration else {
        return div()
            .rounded(px(10.0))
            .border_1()
            .border_color(border)
            .p(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(20.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        Text::new("Automatic directory loading")
                            .size(Size::Sm)
                            .bold(),
                    )
                    .child(
                        Text::new(error.unwrap_or_else(|| {
                            "Synapse could not detect a supported default shell.".to_owned()
                        }))
                        .size(Size::Xs)
                        .dimmed(),
                    ),
            )
            .child(
                Button::new("unavailableshell", "Unavailable")
                    .variant(Variant::Light)
                    .color(ColorName::Gray)
                    .size(Size::Sm)
                    .disabled(true),
            )
            .into_any_element();
    };

    let state = integration.state;
    let description = match state {
        IntegrationState::Missing => format!(
            "Detected {}. Enabling installs the CLI if needed and adds one managed block.",
            integration.shell
        ),
        IntegrationState::Installed => format!(
            "Enabled for {}. New terminal sessions load approved directory scopes automatically.",
            integration.shell
        ),
        IntegrationState::Modified => format!(
            "The managed {} block changed. Repair replaces only that block.",
            integration.shell
        ),
    };
    let path = integration.path.display().to_string();
    let controls = match state {
        IntegrationState::Missing => Button::new("installshell", "Enable shell hook")
            .variant(Variant::Filled)
            .color(ColorName::Violet)
            .size(Size::Sm)
            .left_section(Icon::new(IconName::Plug).size(Size::Xs))
            .on_click(move |event, window, cx| install(event, window, cx))
            .into_any_element(),
        IntegrationState::Installed => Button::new("removeshell", "Remove hook")
            .variant(Variant::Light)
            .color(ColorName::Gray)
            .size(Size::Sm)
            .left_section(Icon::new(IconName::Unplug).size(Size::Xs))
            .on_click(move |event, window, cx| remove(event, window, cx))
            .into_any_element(),
        IntegrationState::Modified => div()
            .flex()
            .gap(px(8.0))
            .child(
                Button::new("repairshell", "Repair hook")
                    .variant(Variant::Filled)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .left_section(Icon::new(IconName::RefreshCw).size(Size::Xs))
                    .on_click(move |event, window, cx| install(event, window, cx)),
            )
            .child(
                Button::new("removemodifiedshell", "Remove")
                    .variant(Variant::Light)
                    .color(ColorName::Gray)
                    .size(Size::Sm)
                    .on_click(move |event, window, cx| remove(event, window, cx)),
            )
            .into_any_element(),
    };

    div()
        .rounded(px(10.0))
        .border_1()
        .border_color(border)
        .p(px(14.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(20.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .child(Icon::new(IconName::Shell).size(Size::Sm))
                                .child(
                                    Text::new("Automatic directory loading")
                                        .size(Size::Sm)
                                        .bold(),
                                ),
                        )
                        .child(Text::new(description).size(Size::Xs).dimmed())
                        .child(Text::new(path).size(Size::Xs).dimmed()),
                )
                .child(controls),
        )
        .child(
            Text::new(
                "Open a new terminal after changing this setting. Existing terminals keep their current hook until closed.",
            )
            .size(Size::Xs)
            .dimmed(),
        )
        .into_any_element()
}

fn shellmode(
    label: &str,
    command: &str,
    description: &str,
    border: gpui::Hsla,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(Text::new(label.to_owned()).size(Size::Sm).bold())
        .child(
            div()
                .rounded(px(8.0))
                .border_1()
                .border_color(border)
                .p(px(10.0))
                .child(Text::new(command.to_owned()).size(Size::Xs)),
        )
        .child(Text::new(description.to_owned()).size(Size::Xs).dimmed())
}

/// One choice in a panel. `panel` is what keeps the ids apart: every switch
/// here labels its two buttons "Off" and "On", and gpui takes the first element
/// with a given id and drops the rest — which is why the mesh switch worked and
/// the two below it did nothing at all.
fn option(
    panel: &str,
    label: &str,
    description: &str,
    selected: bool,
    click: Click,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            Button::new(
                gpui::ElementId::Name(format!("{panel}{}", label.to_lowercase()).into()),
                label.to_owned(),
            )
            .variant(if selected {
                Variant::Filled
            } else {
                Variant::Light
            })
            .color(ColorName::Violet)
            .size(Size::Sm)
            .on_click(move |event, window, cx| click(event, window, cx)),
        )
        .child(Text::new(description.to_owned()).size(Size::Xs).dimmed())
}

fn themebutton(label: &str, icon: IconName, selected: bool, click: Click) -> impl IntoElement {
    Button::new(
        gpui::ElementId::Name(format!("theme{}", label.to_lowercase()).into()),
        label.to_owned(),
    )
    .variant(if selected {
        Variant::Filled
    } else {
        Variant::Light
    })
    .color(ColorName::Violet)
    .size(Size::Sm)
    .left_section(Icon::new(icon).size(Size::Xs))
    .on_click(move |event, window, cx| click(event, window, cx))
}

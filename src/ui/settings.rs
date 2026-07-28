use crate::brain::Optimization;
use crate::cli::InstallStatus;
use crate::shellsetup::{Integration, IntegrationState};
use crate::ui::theme::Mode;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Window, div, px};
use guise::prelude::*;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub optimization: Optimization,
    pub thememode: Mode,
    pub clistatus: InstallStatus,
    pub clipath: String,
    pub shellintegration: Option<Integration>,
    pub shellerror: Option<String>,
    pub message: Option<(String, bool)>,
}

pub struct Actions {
    pub full: Click,
    pub balanced: Click,
    pub lean: Click,
    pub system: Click,
    pub light: Click,
    pub dark: Click,
    pub install: Click,
    pub shellinstall: Click,
    pub shellremove: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Actions {
        full,
        balanced,
        lean,
        system,
        light,
        dark,
        install,
        shellinstall,
        shellremove,
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
                                "Tune what Synapse sends, how it looks, and how it integrates with your shell.",
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
                                    "Full",
                                    "Up to 25 results · original formatting · no response budget",
                                    view.optimization == Optimization::Full,
                                    full,
                                ))
                                .child(option(
                                    "Balanced",
                                    "Up to 8 results · compact whitespace · about 1,500 tokens",
                                    view.optimization == Optimization::Balanced,
                                    balanced,
                                ))
                                .child(option(
                                    "Lean",
                                    "Up to 4 results · deduplicated · about 700 tokens",
                                    view.optimization == Optimization::Lean,
                                    lean,
                                )),
                        )
                        .child(
                            Text::new(
                                "Token counts are estimates; exact usage depends on the connected model's tokenizer.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
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

fn option(label: &str, description: &str, selected: bool, click: Click) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            Button::new(
                gpui::ElementId::Name(format!("optimization{}", label.to_lowercase()).into()),
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

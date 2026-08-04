use crate::ui::Dashboard;
use gpui::{App, Context, Entity, Global, SharedString, WindowHandle};
use guise::prelude::MenuBar;

gpui::actions!(
    synapse,
    [
        OpenSynapse,
        InstallCli,
        OpenData,
        SystemTheme,
        LightTheme,
        DarkTheme,
        QuitSynapse
    ]
);

#[derive(Default)]
pub struct State {
    pub message: Option<String>,
    pub error: bool,
}

impl Global for State {}

pub fn configure(window: WindowHandle<Dashboard>, cx: &mut App) {
    cx.set_global(State::default());
    cx.set_menus(vec![
        gpui::Menu {
            name: SharedString::new_static("Synapse"),
            items: vec![
                gpui::MenuItem::action("Open Synapse", OpenSynapse),
                gpui::MenuItem::action("Install CLI", InstallCli),
                gpui::MenuItem::action("Open Data Folder", OpenData),
                gpui::MenuItem::submenu(gpui::Menu {
                    name: SharedString::new_static("Appearance"),
                    items: vec![
                        gpui::MenuItem::action("System", SystemTheme),
                        gpui::MenuItem::action("Light", LightTheme),
                        gpui::MenuItem::action("Dark", DarkTheme),
                    ],
                }),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Quit Synapse", QuitSynapse),
            ],
        },
        gpui::Menu {
            name: SharedString::new_static("View"),
            items: vec![
                gpui::MenuItem::action("Use System Appearance", SystemTheme),
                gpui::MenuItem::action("Use Light Appearance", LightTheme),
                gpui::MenuItem::action("Use Dark Appearance", DarkTheme),
            ],
        },
    ]);

    let openwindow = window;
    cx.on_action::<OpenSynapse>(move |_, cx| {
        cx.activate(true);
        let _ = openwindow.update(cx, |_view, window, _cx| window.activate_window());
    });
    cx.on_action::<InstallCli>(|_, cx| installcli(cx));
    cx.on_action::<OpenData>(|_, cx| {
        let result = synapsecore::files::data().and_then(|path| synapsecore::files::reveal(&path));
        setresult(
            result.map(|_| "Opened the Synapse data folder.".to_owned()),
            cx,
        );
    });
    cx.on_action::<SystemTheme>(|_, cx| crate::ui::theme::set(crate::ui::theme::Mode::System, cx));
    cx.on_action::<LightTheme>(|_, cx| crate::ui::theme::set(crate::ui::theme::Mode::Light, cx));
    cx.on_action::<DarkTheme>(|_, cx| crate::ui::theme::set(crate::ui::theme::Mode::Dark, cx));
    cx.on_action::<QuitSynapse>(move |_, cx| {
        let canquit = window
            .update(cx, |view, window, cx| view.preparequit(window, cx))
            .unwrap_or(true);
        if canquit {
            cx.quit();
        } else {
            cx.activate(true);
        }
    });
}

pub fn installcli(cx: &mut App) {
    let result =
        synapsecore::cli::install().map(|path| format!("CLI installed at {}.", path.display()));
    setresult(result, cx);
}

pub fn installshell(cx: &mut App) {
    let result = (|| {
        let command = synapsecore::cli::install()?;
        let integration = synapsecore::shellsetup::install(&command)?;
        Ok(format!(
            "{} shell integration enabled in {}. Open a new terminal to use it.",
            integration.shell,
            integration.path.display()
        ))
    })();
    setresult(result, cx);
}

pub fn removeshell(cx: &mut App) {
    let result = (|| {
        let command = synapsecore::cli::destination()?;
        let integration = synapsecore::shellsetup::remove(&command)?;
        Ok(format!(
            "Shell integration removed from {}. Existing terminals keep it until they close.",
            integration.path.display()
        ))
    })();
    setresult(result, cx);
}

pub fn message(cx: &App) -> Option<(String, bool)> {
    cx.try_global::<State>()
        .and_then(|state| state.message.clone().map(|message| (message, state.error)))
}

pub fn bar<T: 'static>(cx: &mut Context<T>) -> Option<Entity<MenuBar>> {
    #[cfg(target_os = "macos")]
    {
        let _ = cx;
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(cx.new(|cx| {
            MenuBar::new(cx)
                .menu("Synapse", |menu| {
                    menu.item("Open Synapse", |_, cx| cx.dispatch_action(&OpenSynapse))
                        .item("Install CLI", |_, cx| cx.dispatch_action(&InstallCli))
                        .item("Open data folder", |_, cx| cx.dispatch_action(&OpenData))
                        .divider()
                        .danger_item("Quit", |_, cx| cx.dispatch_action(&QuitSynapse))
                })
                .menu("View", |menu| {
                    menu.item("System appearance", |_, cx| {
                        cx.dispatch_action(&SystemTheme)
                    })
                    .item("Light appearance", |_, cx| cx.dispatch_action(&LightTheme))
                    .item("Dark appearance", |_, cx| cx.dispatch_action(&DarkTheme))
                })
        }))
    }
}

fn setresult(result: anyhow::Result<String>, cx: &mut App) {
    let (message, error) = match result {
        Ok(message) => (message, false),
        Err(error) => (error.to_string(), true),
    };
    let state = cx.global_mut::<State>();
    state.message = Some(message);
    state.error = error;
    cx.refresh_windows();
}

use crate::ui::{Dashboard, SaveDocument};
use gpui::{App, Context, Entity, Global, KeyBinding, Keystroke, SharedString, WindowHandle};
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
        HideSynapse,
        HideOthers,
        MinimizeWindow,
        ZoomWindow,
        CloseWindow,
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        SelectAll,
        QuitSynapse
    ]
);

#[derive(Default)]
pub struct State {
    pub message: Option<String>,
    pub error: bool,
    /// Set while an Edit item is handing its keystroke back to the window. See
    /// [`forward`].
    forwarding: bool,
}

impl Global for State {}

/// Run something against the window, once the current dispatch has unwound.
///
/// A menu item arrives through a window update of gpui's own making, and a
/// second update of the same window from inside the first is refused — quietly,
/// since the handler only gets a `Result` it has no one to report to. Every one
/// of these would have looked like a menu item that does nothing.
fn withwindow(
    window: WindowHandle<Dashboard>,
    cx: &mut App,
    action: impl FnOnce(&mut Dashboard, &mut gpui::Window, &mut Context<Dashboard>) + 'static,
) {
    cx.defer(move |cx| {
        let _ = window.update(cx, action);
    });
}

/// The Edit menu's keystrokes, and the field they belong to.
///
/// A guise input reads keys, not actions — cut is `cmd-x` arriving at whatever
/// has focus, and there is no method on the entity to call instead. An enabled
/// menu item carrying ⌘X takes that keystroke before the window ever sees it,
/// so the item's job is to put it back: dispatch the same keystroke into the
/// window and let the focused field do what it already knows how to do.
///
/// The flag is what stops that from looping. The keystroke comes back through
/// the keymap and lands on this same action, and the second time through the
/// handler asks gpui to keep propagating rather than forwarding again — which
/// is exactly what lets it reach the key listeners underneath.
fn forward(keystroke: &'static str, window: WindowHandle<Dashboard>, cx: &mut App) {
    if cx.global::<State>().forwarding {
        cx.propagate();
        return;
    }
    let Ok(stroke) = Keystroke::parse(keystroke) else {
        return;
    };
    cx.global_mut::<State>().forwarding = true;
    // The menu dispatches inside a window update of its own, so the keystroke
    // has to wait for that to unwind before it can open another one.
    cx.defer(move |cx| {
        let _ = window.update(cx, |_view, window, cx| {
            window.dispatch_keystroke(stroke, cx);
        });
        cx.global_mut::<State>().forwarding = false;
    });
}

pub fn configure(window: WindowHandle<Dashboard>, cx: &mut App) {
    cx.set_global(State::default());

    // Before the menus: `set_menus` reads the keymap to put the ⌘-glyph on each
    // item, so a binding registered afterwards is a shortcut the menu never
    // shows and macOS never claims.
    cx.bind_keys([
        KeyBinding::new("cmd-s", SaveDocument, None),
        KeyBinding::new("ctrl-s", SaveDocument, None),
        KeyBinding::new("cmd-h", HideSynapse, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", QuitSynapse, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-a", SelectAll, None),
    ]);

    cx.set_menus(vec![
        gpui::Menu {
            name: SharedString::new_static("Synapse"),
            items: vec![
                gpui::MenuItem::action("Open Synapse", OpenSynapse),
                gpui::MenuItem::action("Install CLI", InstallCli),
                gpui::MenuItem::action("Open Data Folder", OpenData),
                gpui::MenuItem::separator(),
                gpui::MenuItem::submenu(gpui::Menu {
                    name: SharedString::new_static("Appearance"),
                    items: vec![
                        gpui::MenuItem::action("System", SystemTheme),
                        gpui::MenuItem::action("Light", LightTheme),
                        gpui::MenuItem::action("Dark", DarkTheme),
                    ],
                }),
                gpui::MenuItem::separator(),
                gpui::MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Hide Synapse", HideSynapse),
                gpui::MenuItem::action("Hide Others", HideOthers),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Quit Synapse", QuitSynapse),
            ],
        },
        gpui::Menu {
            name: SharedString::new_static("File"),
            items: vec![
                gpui::MenuItem::action("Save", SaveDocument),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Close Window", CloseWindow),
            ],
        },
        gpui::Menu {
            name: SharedString::new_static("Edit"),
            items: vec![
                gpui::MenuItem::os_action("Undo", Undo, gpui::OsAction::Undo),
                gpui::MenuItem::os_action("Redo", Redo, gpui::OsAction::Redo),
                gpui::MenuItem::separator(),
                gpui::MenuItem::os_action("Cut", Cut, gpui::OsAction::Cut),
                gpui::MenuItem::os_action("Copy", Copy, gpui::OsAction::Copy),
                gpui::MenuItem::os_action("Paste", Paste, gpui::OsAction::Paste),
                gpui::MenuItem::separator(),
                gpui::MenuItem::os_action("Select All", SelectAll, gpui::OsAction::SelectAll),
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
        gpui::Menu {
            name: SharedString::new_static("Window"),
            items: vec![
                gpui::MenuItem::action("Minimize", MinimizeWindow),
                gpui::MenuItem::action("Zoom", ZoomWindow),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Bring Synapse to Front", OpenSynapse),
            ],
        },
    ]);

    let openwindow = window;
    cx.on_action::<OpenSynapse>(move |_, cx| {
        show(openwindow, cx);
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
    cx.on_action::<HideSynapse>(|_, cx| cx.hide());
    cx.on_action::<HideOthers>(|_, cx| cx.hide_other_apps());
    cx.on_action::<MinimizeWindow>(move |_, cx| {
        withwindow(window, cx, |_view, window, _cx| window.minimize_window());
    });
    cx.on_action::<ZoomWindow>(move |_, cx| {
        withwindow(window, cx, |_view, window, _cx| window.zoom_window());
    });
    // ⌘W is the red button, and the red button has always put Synapse away
    // rather than ending it: the window comes back from the menu bar item.
    cx.on_action::<CloseWindow>(move |_, cx| dismiss(cx));
    cx.on_action::<Undo>(move |_, cx| forward("cmd-z", window, cx));
    cx.on_action::<Redo>(move |_, cx| forward("cmd-shift-z", window, cx));
    cx.on_action::<Cut>(move |_, cx| forward("cmd-x", window, cx));
    cx.on_action::<Copy>(move |_, cx| forward("cmd-c", window, cx));
    cx.on_action::<Paste>(move |_, cx| forward("cmd-v", window, cx));
    cx.on_action::<SelectAll>(move |_, cx| forward("cmd-a", window, cx));
    cx.on_action::<QuitSynapse>(move |_, cx| quit(window, cx));
}

/// Bring the window back, from wherever it was asked for: the menu, the menu
/// bar item, or a second launch. Both surfaces go through here so the Dock icon
/// and the menu bar come back with it.
pub fn show(window: WindowHandle<Dashboard>, cx: &mut App) {
    #[cfg(target_os = "macos")]
    crate::ui::statusbar::foreground();
    cx.activate(true);
    withwindow(window, cx, |_view, window, _cx| window.activate_window());
}

/// Put the window away. Synapse keeps running in the menu bar, so it drops back
/// to an accessory: no Dock icon, and no menu bar belonging to a window that is
/// not on screen.
pub fn dismiss(cx: &mut App) {
    cx.hide();
    #[cfg(target_os = "macos")]
    crate::ui::statusbar::background();
}

/// Quitting is the one thing an unsaved document may refuse, which is why this
/// asks the window first rather than calling `cx.quit()` where it stands.
pub fn quit(window: WindowHandle<Dashboard>, cx: &mut App) {
    cx.defer(move |cx| {
        let canquit = window
            .update(cx, |view, window, cx| view.preparequit(window, cx))
            .unwrap_or(true);
        if canquit {
            cx.quit();
        } else {
            show(window, cx);
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
                .menu("Edit", |menu| {
                    menu.item("Undo", |_, cx| cx.dispatch_action(&Undo))
                        .item("Redo", |_, cx| cx.dispatch_action(&Redo))
                        .divider()
                        .item("Cut", |_, cx| cx.dispatch_action(&Cut))
                        .item("Copy", |_, cx| cx.dispatch_action(&Copy))
                        .item("Paste", |_, cx| cx.dispatch_action(&Paste))
                        .divider()
                        .item("Select all", |_, cx| cx.dispatch_action(&SelectAll))
                })
                .menu("View", |menu| {
                    menu.item("System appearance", |_, cx| {
                        cx.dispatch_action(&SystemTheme)
                    })
                    .item("Light appearance", |_, cx| cx.dispatch_action(&LightTheme))
                    .item("Dark appearance", |_, cx| cx.dispatch_action(&DarkTheme))
                })
                .menu("Window", |menu| {
                    menu.item("Minimize", |_, cx| cx.dispatch_action(&MinimizeWindow))
                        .item("Zoom", |_, cx| cx.dispatch_action(&ZoomWindow))
                        .divider()
                        .item("Close window", |_, cx| cx.dispatch_action(&CloseWindow))
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

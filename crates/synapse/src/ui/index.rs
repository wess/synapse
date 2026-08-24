use crate::ui::{Dashboard, SaveDocument};
use gpui::{
    App, AppContext, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

pub fn run() {
    // The window is the one place a crash has nowhere to be seen, so write it
    // down before the process goes.
    synapsecore::crashes::capture();
    gpui::Application::new().run(|cx: &mut App| {
        crate::ui::theme::initialize(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-s", SaveDocument, None),
            KeyBinding::new("ctrl-s", SaveDocument, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1240.0), px(680.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(SharedString::new_static("Synapse")),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    crate::ui::theme::sync(window, cx);
                    window
                        .observe_window_appearance(crate::ui::theme::sync)
                        .detach();
                    window.on_window_should_close(cx, |_window, cx| {
                        cx.hide();
                        false
                    });
                    let dashboard = cx.new(Dashboard::new);
                    dashboard.update(cx, |dashboard, cx| dashboard.opened(cx));
                    dashboard
                },
            )
            .expect("open Synapse window");
        crate::ui::menu::configure(window, cx);

        #[cfg(target_os = "macos")]
        {
            use crate::ui::statusbar::{self, Action};
            use gpui::Task;

            let (sender, receiver) = async_channel::unbounded();
            // The menu bar item is a convenience, and the window is already
            // open. Losing it is worth a line on the console, not the app.
            let statusbar = match statusbar::install(sender) {
                Ok(statusbar) => Some(statusbar),
                Err(error) => {
                    eprintln!("Synapse could not add its menu bar item: {error:#}");
                    None
                }
            };
            if let Some(statusbar) = statusbar {
                cx.set_global(statusbar);
            }
            let data = synapsecore::files::data().ok();
            let task: Task<()> = cx.spawn(async move |cx| {
                while let Ok(action) = receiver.recv().await {
                    let _ = cx.update(|cx| match action {
                        Action::Open => {
                            cx.activate(true);
                            let _ = window.update(cx, |_view, window, _cx| {
                                window.activate_window();
                            });
                        }
                        Action::Data => {
                            if let Some(path) = data.as_deref() {
                                let _ = synapsecore::files::reveal(path);
                            }
                        }
                        Action::Install => crate::ui::menu::installcli(cx),
                        Action::SystemTheme => {
                            crate::ui::theme::set(crate::ui::theme::Mode::System, cx)
                        }
                        Action::LightTheme => {
                            crate::ui::theme::set(crate::ui::theme::Mode::Light, cx)
                        }
                        Action::DarkTheme => {
                            crate::ui::theme::set(crate::ui::theme::Mode::Dark, cx)
                        }
                        Action::Quit => {
                            let canquit = window
                                .update(cx, |view, window, cx| view.preparequit(window, cx))
                                .unwrap_or(true);
                            if canquit {
                                cx.quit();
                            } else {
                                cx.activate(true);
                            }
                        }
                    });
                }
            });
            task.detach();
        }
        cx.activate(true);
    });
}

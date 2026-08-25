use crate::ui::Dashboard;
use gpui::{
    App, AppContext, Bounds, SharedString, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

pub fn run() {
    // The window is the one place a crash has nowhere to be seen, so write it
    // down before the process goes.
    synapsecore::crashes::capture();
    gpui::Application::new().run(|cx: &mut App| {
        crate::ui::theme::initialize(cx);

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
                        crate::ui::menu::dismiss(cx);
                        false
                    });
                    let dashboard = cx.new(Dashboard::new);
                    dashboard.update(cx, |dashboard, cx| dashboard.opened(cx));
                    dashboard
                },
            )
            .expect("open Synapse window");
        // The menus carry the keymap with them, so they are configured before
        // anything can be typed at the window.
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
                    // Every one of these has a twin in the menu bar. They go
                    // through the same functions so the two cannot come to mean
                    // different things.
                    let _ = cx.update(|cx| match action {
                        Action::Open => crate::ui::menu::show(window, cx),
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
                        Action::Quit => crate::ui::menu::quit(window, cx),
                    });
                }
            });
            task.detach();
        }
        crate::ui::menu::show(window, cx);
    });
}

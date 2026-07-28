use crate::ui::{Dashboard, SaveDocument};
use gpui::{
    App, AppContext, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

pub fn run() {
    gpui::Application::new().run(|cx: &mut App| {
        crate::ui::theme::initialize(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-s", SaveDocument, None),
            KeyBinding::new("ctrl-s", SaveDocument, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1020.0), px(600.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(SharedString::new_static("Synaps")),
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
                    cx.new(Dashboard::new)
                },
            )
            .expect("open Synaps window");
        crate::ui::menu::configure(window, cx);

        #[cfg(target_os = "macos")]
        {
            use crate::ui::statusbar::{self, Action};
            use gpui::Task;

            let (sender, receiver) = async_channel::unbounded();
            let statusbar = statusbar::install(sender).expect("create Synaps status item");
            cx.set_global(statusbar);
            let data = crate::files::data().ok();
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
                                let _ = crate::files::reveal(path);
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

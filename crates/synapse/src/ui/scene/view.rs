mod components;

use super::*;
use components::{details, filters, label, projectname, selector};
use gpui::prelude::*;
use gpui::{IntoElement, Render, Window, div, img, px};
use guise::prelude::*;

impl Render for Scene {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = guise::theme(cx);
        let focuscolor = theme.primary().hsla();
        let bordercolor = theme.border().hsla();
        let surfacecolor = theme.surface().hsla();
        let time = presence::now();
        let online = self
            .sessions
            .iter()
            .filter(|session| session.online(time))
            .count();
        let visible: Vec<_> = self
            .sessions
            .iter()
            .filter(|session| {
                self.project
                    .as_ref()
                    .is_none_or(|project| &session.project == project)
            })
            .cloned()
            .collect();
        let selected = self
            .sessions
            .iter()
            .find(|session| Some(&session.id) == self.selected.as_ref())
            .cloned();
        let mut projects: Vec<_> = self
            .sessions
            .iter()
            .map(|session| session.project.clone())
            .collect();
        projects.sort();
        projects.dedup();
        let selector = selector(&self.plugins, self.selectedplugin, cx);
        let mut rooms = div()
            .flex_1()
            .min_w(px(292.0))
            .flex()
            .flex_wrap()
            .items_start()
            .content_start()
            .gap(px(18.0));
        for session in &visible {
            let id = session.id.clone();
            let keyid = id.clone();
            let state = session.state(time);
            let picture = self.picture(session.appearance as usize, state, true);
            let selected = Some(&session.id) == self.selected.as_ref();
            rooms = rooms.child(
                div()
                    .id(gpui::ElementId::Name(format!("room{}", session.id).into()))
                    .w(px(292.0))
                    .border_2()
                    .border_color(if selected { focuscolor } else { bordercolor })
                    .bg(surfacecolor)
                    .cursor_pointer()
                    .focusable()
                    .focus(move |style| style.border_color(focuscolor))
                    .tab_index(0)
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            this.selected = Some(keyid.clone());
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(id.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(8.0))
                            .flex()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                Text::new(format!("{} · {}", session.label(), session.slot + 1))
                                    .size(Size::Sm),
                            )
                            .child(Text::new(label(state)).size(Size::Xs).dimmed()),
                    )
                    .child(img(picture).w(px(288.0)).h(px(208.0)))
                    .child(
                        div().px(px(10.0)).py(px(9.0)).child(
                            Text::new(projectname(&session.project))
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                    ),
            );
        }
        if visible.is_empty() {
            let picture = self.picture(0, State::Connected, false);
            rooms = rooms.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(img(picture).w(px(288.0)).h(px(208.0)))
                    .child(Title::new("A room for every session").order(3))
                    .child(
                        Text::new("Start or restart a connected harness to bring its character into the workshop. Agent mesh can stay off.")
                            .size(Size::Sm),
                    )
                    .child(
                        Text::new("Working, waiting, and blocked animations follow reported status. A connection alone shows a resting character.")
                            .size(Size::Xs)
                            .dimmed(),
                    ),
            );
        }
        let details = details(selected, time, cx);
        let filters = filters(projects, self.project.as_ref(), cx);
        div()
            .id("scenemain")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(
                div()
                    .p(px(28.0))
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(Title::new("Scenes").order(2))
                                    .child(
                                        Text::new(format!(
                                            "{online} connected {}",
                                            if online == 1 { "session" } else { "sessions" }
                                        ))
                                        .size(Size::Sm)
                                        .dimmed(),
                                    ),
                            )
                            .child(
                                Button::new(
                                    "scenepause",
                                    if self.paused {
                                        "Resume animation"
                                    } else {
                                        "Pause animation"
                                    },
                                )
                                .size(Size::Sm)
                                .variant(Variant::Subtle)
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.paused = !this.paused;
                                        this.save("scene.paused", this.paused.to_string(), cx);
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(selector)
                    .child(filters)
                    .children(
                        self.error
                            .iter()
                            .chain(self.presenceerror.iter())
                            .chain(self.problems.iter())
                            .map(|error| Text::new(error.clone()).size(Size::Xs)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(24.0))
                            .items_start()
                            .child(rooms)
                            .child(details),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Button::new("sceneplugins", "Open plugins folder")
                                    .size(Size::Xs)
                                    .variant(Variant::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Err(error) = plugins::folder()
                                            .and_then(|path| synapsecore::files::reveal(&path))
                                        {
                                            this.error = Some(error.to_string());
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                Button::new("scenereload", "Reload plugins")
                                    .size(Size::Xs)
                                    .variant(Variant::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                            )
                            .child(
                                Text::new("Local scene packs · API 1")
                                    .size(Size::Xs)
                                    .dimmed(),
                            ),
                    ),
            )
    }
}

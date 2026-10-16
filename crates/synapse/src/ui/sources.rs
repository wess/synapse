//! Where a search looks, under the results it found.
//!
//! The same panel on both Search tabs, each showing the sources that hold what
//! that tab searches for. Adding one is a name and a location — `owner/repo`
//! or an address — and Synapse works out the kind, which is the one question a
//! person should not have to answer.

use crate::ui::skills::Click;
use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, Hsla, div, px};
use guise::prelude::*;
use synapsecore::sources::{Origin, Source};

pub struct View {
    pub sources: Vec<Source>,
    pub problems: Vec<String>,
    pub name: Entity<TextInput>,
    pub location: Entity<TextInput>,
    pub busy: bool,
}

pub struct Actions {
    pub add: Click,
    pub remove: Box<dyn Fn(String) -> Click>,
    pub allow: Click,
}

pub fn render(prefix: &'static str, view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border: Hsla = theme.border().hsla();
    let surface: Hsla = theme.surface().hsla();
    let Actions { add, remove, allow } = actions;
    let pending = view
        .sources
        .iter()
        .any(|source| matches!(source.origin, Origin::Project { trusted: false, .. }));
    let mut allow = Some(allow);

    let mut panel = div()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(Text::new("Sources").size(Size::Sm).bold())
                .child(div().flex_none().w(px(1.0))),
        );

    for (index, source) in view.sources.into_iter().enumerate() {
        let (layer, color) = match &source.origin {
            Origin::Builtin => ("built in", ColorName::Gray),
            Origin::User => ("yours", ColorName::Violet),
            Origin::Project { trusted: true, .. } => ("this project", ColorName::Blue),
            Origin::Project { trusted: false, .. } => ("not approved", ColorName::Orange),
        };
        let removable = matches!(source.origin, Origin::Builtin | Origin::User);
        let mut row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Text::new(source.name.clone()).size(Size::Xs).bold())
                    .child(
                        Badge::new(source.kind.label())
                            .size(Size::Sm)
                            .color(ColorName::Gray),
                    )
                    .child(Badge::new(layer).size(Size::Sm).color(color))
                    .child(Text::new(source.location()).size(Size::Xs).dimmed()),
            );
        if removable {
            let click = remove(source.name.clone());
            row = row.child(
                Button::new((prefix, index), "Remove")
                    .variant(Variant::Subtle)
                    .color(ColorName::Red)
                    .size(Size::Xs)
                    .on_click(move |event, window, cx| click(event, window, cx)),
            );
        } else if pending
            && matches!(source.origin, Origin::Project { trusted: false, .. })
            && let Some(click) = allow.take()
        {
            // One approval covers the file, so the button is offered once.
            row = row.child(
                Button::new((prefix, index), "Allow this project's sources")
                    .variant(Variant::Light)
                    .color(ColorName::Orange)
                    .size(Size::Xs)
                    .on_click(move |event, window, cx| click(event, window, cx)),
            );
        }
        panel = panel.child(row);
    }

    for problem in view.problems {
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(theme.danger().hsla())
                .child(Icon::new(IconName::CircleAlert).size(Size::Xs))
                .child(Text::new(problem).size(Size::Xs)),
        );
    }

    panel
        .child(
            div()
                .flex()
                .items_end()
                .gap(px(8.0))
                .child(div().w(px(160.0)).flex_none().flex().flex_col().child(view.name))
                .child(div().flex_1().min_w(px(0.0)).flex().flex_col().child(view.location))
                .child(
                    Button::new((prefix, 9999_usize), if view.busy { "Adding…" } else { "Add source" })
                        .variant(Variant::Light)
                        .color(ColorName::Violet)
                        .size(Size::Sm)
                        .disabled(view.busy)
                        .left_section(Icon::new(IconName::Plus).size(Size::Xs))
                        .on_click(move |event, window, cx| add(event, window, cx)),
                ),
        )
        .child(
            div().flex().child(
                div().flex_1().min_w(px(0.0)).child(
                    Text::new("A private repository reads its token from the vault: synapse source add <name> <owner/repo> --token <vault.name>.")
                        .size(Size::Xs)
                        .dimmed(),
                ),
            ),
        )
        .into_any_element()
}

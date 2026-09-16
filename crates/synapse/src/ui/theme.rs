use gpui::{App, Global, Window, WindowAppearance};
use guise::prelude::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

pub struct Preference(pub Mode);

impl Global for Preference {}

pub fn initialize(cx: &mut App) {
    cx.set_global(Preference(load().unwrap_or(Mode::System)));
    apply(cx.window_appearance(), cx);
}

pub fn sync(window: &mut Window, cx: &mut App) {
    if cx.global::<Preference>().0 == Mode::System {
        apply(window.appearance(), cx);
        cx.refresh_windows();
    }
}

pub fn set(mode: Mode, cx: &mut App) {
    cx.global_mut::<Preference>().0 = mode;
    let _ = save(mode);
    apply(cx.window_appearance(), cx);
    cx.refresh_windows();
}

pub fn mode(cx: &App) -> Mode {
    cx.global::<Preference>().0
}

fn apply(appearance: WindowAppearance, cx: &mut App) {
    let mode = cx.global::<Preference>().0;
    let dark = match mode {
        Mode::System => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
        Mode::Light => false,
        Mode::Dark => true,
    };
    if dark {
        darktheme().init(cx);
    } else {
        lighttheme().init(cx);
    }
}

fn lighttheme() -> Theme {
    Theme::light()
        .with_primary(guise::rgb(103, 82, 176))
        .with_body(guise::rgb(242, 243, 246))
        .with_surface(guise::rgb(250, 250, 251))
        .with_surface_hover(guise::rgb(232, 234, 240))
        .with_text(guise::rgb(31, 29, 36))
        .with_dimmed(guise::rgb(97, 93, 108))
        .with_border(guise::rgb(211, 214, 222))
        .with_success(guise::rgb(29, 126, 109))
        .with_danger(guise::rgb(187, 53, 69))
}

fn darktheme() -> Theme {
    Theme::dark()
        .with_primary(guise::rgb(154, 132, 230))
        .with_body(guise::rgb(25, 27, 32))
        .with_surface(guise::rgb(31, 34, 40))
        .with_surface_hover(guise::rgb(45, 49, 58))
        .with_text(guise::rgb(240, 238, 244))
        .with_dimmed(guise::rgb(175, 169, 187))
        .with_border(guise::rgb(61, 66, 77))
        .with_success(guise::rgb(83, 190, 162))
        .with_danger(guise::rgb(235, 113, 128))
}

fn load() -> anyhow::Result<Mode> {
    crate::ui::runtime::block(async {
        // `glance` rather than `open`: this runs before the window exists and
        // reads one preference, and a whole-page integrity scan on the way to
        // finding out whether somebody prefers dark mode is the wrong trade.
        let brain = synapsecore::brain::Brain::glance(synapsecore::files::database()?).await?;
        Ok(match brain.preference("appearance").await?.as_deref() {
            Some("light") => Mode::Light,
            Some("dark") => Mode::Dark,
            _ => Mode::System,
        })
    })
}

fn save(mode: Mode) -> anyhow::Result<()> {
    let value = match mode {
        Mode::System => "system",
        Mode::Light => "light",
        Mode::Dark => "dark",
    };
    crate::ui::runtime::block(async {
        let brain = synapsecore::brain::Brain::open(synapsecore::files::database()?).await?;
        brain.setpreference("appearance", value).await
    })
}

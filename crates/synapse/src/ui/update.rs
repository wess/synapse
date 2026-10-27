//! Synapse's Guise updater configuration.

use crate::ui::Dashboard;
use gpui::{App, WindowHandle};
use guise::update::Updater;

const TEAM: &str = "XJDC46F35X";

fn updater(dashboard: WindowHandle<Dashboard>) -> Updater {
    Updater::github("Synapse", env!("CARGO_PKG_VERSION"), "wess/synapse")
        .codesign_requirement(format!(
            "anchor apple generic and certificate leaf[subject.OU] = {TEAM}"
        ))
        .window_title("Synapse Update")
        .restart_guard(move |cx| {
            dashboard
                .update(cx, |view, window, cx| view.preparequit(window, cx))
                .unwrap_or(true)
        })
}

/// Check on launch and then at Guise's bounded polling interval.
pub fn start(dashboard: WindowHandle<Dashboard>, cx: &mut App) {
    guise::update::start(updater(dashboard), cx);
}

/// Answer a user-requested check even when there is nothing to install.
pub fn check_now(dashboard: WindowHandle<Dashboard>, cx: &mut App) {
    guise::update::check_now(updater(dashboard), cx);
}

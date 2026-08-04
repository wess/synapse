//! The desktop palette, as far as a terminal can carry it.
//!
//! The app's canvas colours do not come across: a terminal owns its own
//! background, and painting over it fights the user's theme instead of matching
//! it. What does come across is the part that carries meaning — the violet
//! accent, the dimmed secondary text, and the success and danger pair — so the
//! two surfaces agree about what a selected row or a failed action looks like.
//!
//! The values are the dark theme's from `ui/theme.rs`, chosen over the light
//! one because they keep enough contrast on either background; a terminal does
//! not tell us which it has.

use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Rgb(154, 132, 230);
pub const SUCCESS: Color = Color::Rgb(83, 190, 162);
pub const DANGER: Color = Color::Rgb(235, 113, 128);
pub const DIM: Color = Color::Rgb(140, 136, 152);
pub const BORDER: Color = Color::Rgb(90, 86, 102);

/// Body text. Deliberately the terminal's own foreground rather than a colour:
/// the one thing every terminal is already right about is what plain text
/// should look like.
pub fn text() -> Style {
    Style::default()
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn heading() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}

/// The selected row. Reversed rather than tinted, because a background colour
/// that assumes a dark terminal disappears on a light one, and selection is the
/// one thing that must never be ambiguous.
pub fn selected() -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::REVERSED)
        .add_modifier(Modifier::BOLD)
}

pub fn good() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn bad() -> Style {
    Style::default().fg(DANGER)
}

/// Status must be readable without relying on colour alone — the same rule the
/// desktop follows, and more important here, where a terminal may have no
/// colour at all.
pub fn mark(ok: bool) -> &'static str {
    if ok { "●" } else { "○" }
}

//! Colour as a second channel, never the first. Every distinction also carries
//! a glyph (`!` / `·`) and a word, so the output reads fully in monochrome.
//!
//! Painted with the 16 ANSI colour names, not fixed RGB: the user's terminal
//! theme maps them, so the palette stays legible on a light or a dark ground
//! without xray guessing the background. anstream (in main) keeps the codes for
//! a terminal and strips them for a pipe. Axis is cyan ↔ yellow ↔ gray plus
//! bold — no red-versus-green as the load-bearing signal.

use anstyle::{AnsiColor, Color, Style};

const fn fg(c: AnsiColor) -> Style {
    Style::new().fg_color(Some(Color::Ansi(c)))
}

/// Register titles and column letters — the report's own structure. Never a severity.
pub const HEADER: Style = fg(AnsiColor::Cyan).bold();
pub const ACCENT: Style = fg(AnsiColor::Cyan);
/// Correctness — damage that corrupts a later step.
pub const CRIT: Style = fg(AnsiColor::Red).bold();
/// Type safety — leading zeros, currency text, mixed types.
pub const WARN: Style = fg(AnsiColor::Yellow);
/// Structure & schema notes.
pub const NOTE: Style = fg(AnsiColor::BrightBlack);
/// De-emphasised detail and labels.
pub const FAINT: Style = fg(AnsiColor::BrightBlack);

/// Wrap text in a style. Emits ANSI unconditionally; anstream decides whether
/// the codes reach the terminal.
pub fn paint(style: Style, text: &str) -> String {
    format!("{}{}{}", style.render(), text, style.render_reset())
}

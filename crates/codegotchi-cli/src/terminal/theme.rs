use ratatui::style::{Color, Style};

/// The four semantic tones used by the terminal room art.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SemanticTone {
    /// Terminal default background / empty space.
    #[default]
    Tone0,
    /// Dark/mid-low silhouette fill.
    Tone1,
    /// Mid-high detail.
    Tone2,
    /// Foreground/highlight.
    Tone3,
}

/// Maps each semantic tone to a concrete Ratatui style.
///
/// `Auto` uses the terminal's default palette so the same art remains
/// readable on dark and light terminals: `Tone3` is the default foreground,
/// `Tone1`/`Tone2` are the neutral grayscale steps, and `Tone0` is the
/// default background. This is the primitive Auto baseline; ordered dithering
/// and named presets are deferred visual polish
/// (`VISUAL_FIDELITY_UNVERIFIED`).
#[must_use]
pub fn auto_style(tone: SemanticTone) -> Style {
    match tone {
        SemanticTone::Tone0 => Style::default().bg(Color::Reset),
        SemanticTone::Tone1 => Style::default().fg(Color::DarkGray),
        SemanticTone::Tone2 => Style::default().fg(Color::Gray),
        SemanticTone::Tone3 => Style::default().fg(Color::Reset),
    }
}

use std::{fmt, str::FromStr};

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

/// A named terminal palette selected by the launcher.
///
/// Presets are intentionally presentation-only: they change the concrete
/// colors used by the room renderer but never affect the authoritative pet
/// snapshot or its care transitions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalThemePreset {
    /// Follow the terminal's own foreground/background while retaining a
    /// coordinate-aware ordered-dither ladder for intermediates.
    #[default]
    Auto,
    /// A four-step neutral monochrome palette with a black room background.
    Mono,
    /// The soft green palette used by the canonical room artwork.
    SoftGreen,
    /// A warm amber terminal palette.
    Amber,
    /// A cool, high-contrast night palette.
    Night,
}

impl TerminalThemePreset {
    /// Resolves this named preset into concrete styles for all four semantic
    /// tones. Resolution happens once at the room/session boundary; draw sites
    /// only consume the resulting palette.
    #[must_use]
    pub fn resolve(self) -> ResolvedPalette {
        match self {
            Self::Auto => ResolvedPalette::auto([
                Style::default().bg(Color::Reset),
                Style::default().fg(Color::Reset).bg(Color::Reset),
                Style::default().fg(Color::Reset).bg(Color::Reset),
                Style::default().fg(Color::Reset),
            ]),
            Self::Mono => ResolvedPalette::new([
                Style::default().bg(Color::Rgb(8, 8, 8)),
                Style::default().fg(Color::Rgb(72, 72, 72)),
                Style::default().fg(Color::Rgb(156, 156, 156)),
                Style::default().fg(Color::Rgb(236, 236, 236)),
            ]),
            Self::SoftGreen => ResolvedPalette::new([
                Style::default().bg(Color::Rgb(7, 15, 12)),
                Style::default().fg(Color::Rgb(24, 74, 45)),
                Style::default().fg(Color::Rgb(96, 166, 112)),
                Style::default().fg(Color::Rgb(166, 220, 177)),
            ]),
            Self::Amber => ResolvedPalette::new([
                Style::default().bg(Color::Rgb(18, 12, 4)),
                Style::default().fg(Color::Rgb(112, 64, 16)),
                Style::default().fg(Color::Rgb(196, 126, 38)),
                Style::default().fg(Color::Rgb(255, 212, 112)),
            ]),
            Self::Night => ResolvedPalette::new([
                Style::default().bg(Color::Rgb(6, 10, 24)),
                Style::default().fg(Color::Rgb(32, 64, 128)),
                Style::default().fg(Color::Rgb(92, 132, 204)),
                Style::default().fg(Color::Rgb(202, 220, 255)),
            ]),
        }
    }

    /// Returns the stable command-line spelling of this preset.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mono => "mono",
            Self::SoftGreen => "soft-green",
            Self::Amber => "amber",
            Self::Night => "night",
        }
    }
}

impl fmt::Display for TerminalThemePreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Error returned when a terminal theme command-line value is unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalThemeParseError {
    value: String,
}

impl fmt::Display for TerminalThemeParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unsupported terminal theme `{}`", self.value)
    }
}

impl std::error::Error for TerminalThemeParseError {}

impl FromStr for TerminalThemePreset {
    type Err = TerminalThemeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "mono" => Ok(Self::Mono),
            "soft-green" => Ok(Self::SoftGreen),
            "amber" => Ok(Self::Amber),
            "night" => Ok(Self::Night),
            value => Err(TerminalThemeParseError {
                value: value.to_owned(),
            }),
        }
    }
}

/// Concrete style mapping for the four semantic art tones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPalette {
    tones: [Style; 4],
    adaptive: bool,
}

impl ResolvedPalette {
    const fn new(tones: [Style; 4]) -> Self {
        Self {
            tones,
            adaptive: false,
        }
    }

    const fn auto(tones: [Style; 4]) -> Self {
        Self {
            tones,
            adaptive: true,
        }
    }

    /// Returns the concrete Ratatui style for one semantic tone.
    #[must_use]
    pub fn style(self, tone: SemanticTone) -> Style {
        self.tones[tone_index(tone)]
    }

    /// Samples one logical art pixel without producing a concrete color.
    ///
    /// Fixed presets preserve their authored tone. Auto maps the endpoints to
    /// terminal defaults and thresholds intermediate tones with a deterministic
    /// 4x4 Bayer pattern so Tone1 has lower default-foreground coverage than
    /// Tone2.
    #[must_use]
    pub fn sample_logical_tone(
        self,
        tone: SemanticTone,
        logical_x: u16,
        logical_y: u16,
    ) -> SemanticTone {
        if !self.adaptive {
            return tone;
        }
        match tone {
            SemanticTone::Tone0 | SemanticTone::Tone3 => tone,
            SemanticTone::Tone1 | SemanticTone::Tone2 => {
                let threshold = BAYER_MATRIX[(logical_y % 4) as usize][(logical_x % 4) as usize];
                let foreground_density = match tone {
                    SemanticTone::Tone1 => 5,
                    _ => 10,
                };
                if threshold < foreground_density {
                    SemanticTone::Tone3
                } else {
                    SemanticTone::Tone0
                }
            }
        }
    }

    /// Returns a cell-ready style that keeps the preset's room background
    /// behind foreground glyphs as well as empty cells.
    #[must_use]
    pub fn cell_style(self, tone: SemanticTone) -> Style {
        let mut style = self.style(tone);
        if style.bg.is_none() {
            style.bg = Some(self.background());
        }
        style
    }

    /// Returns the concrete background color used for empty room cells.
    #[must_use]
    pub fn background(self) -> Color {
        self.style(SemanticTone::Tone0).bg.unwrap_or(Color::Reset)
    }
}

const fn tone_index(tone: SemanticTone) -> usize {
    match tone {
        SemanticTone::Tone0 => 0,
        SemanticTone::Tone1 => 1,
        SemanticTone::Tone2 => 2,
        SemanticTone::Tone3 => 3,
    }
}

/// Maps each semantic tone to a concrete Ratatui style.
///
/// `Auto` uses the terminal's default palette so the same art remains
/// readable on dark and light terminals: `Tone3` is the default foreground,
/// `Tone1`/`Tone2` are the neutral grayscale steps, and `Tone0` is the
/// default background. This compatibility wrapper delegates to the resolved
/// Auto palette so existing callers retain their original semantics.
#[must_use]
pub fn auto_style(tone: SemanticTone) -> Style {
    TerminalThemePreset::Auto.resolve().style(tone)
}

/// Classic normalized 4x4 ordered-dither thresholds (0..16).
const BAYER_MATRIX: [[u16; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

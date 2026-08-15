//! Logical-pixel sprite canvas for the terminal room.
//!
//! Sprites are authored as rectangular grids of *logical pixels*, each one a
//! [`SemanticTone`]. Two logical pixels (top/bottom) pack into one terminal
//! row using the half-block glyphs `▀` (upper pixel) and `▄` (lower pixel),
//! or `█` when both pixels share a tone. This gives two vertical logical
//! pixels per terminal row as the design requires.

use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
};

use super::behavior::PetPose;
use super::theme::{SemanticTone, auto_style};

/// Maps a sprite character to a semantic tone.
/// `' '` = background, `'.'` = Tone1, `'o'` = Tone2, `'#'` = Tone3.
fn tone_for(character: char) -> SemanticTone {
    match character {
        '.' => SemanticTone::Tone1,
        'o' => SemanticTone::Tone2,
        '#' => SemanticTone::Tone3,
        _ => SemanticTone::Tone0,
    }
}

/// Draws a logical-pixel sprite at room-relative `(x, y)`.
///
/// Packing always uses two vertical logical pixels per terminal row.
pub fn draw_sprite<R: AsRef<str>>(area: Rect, buffer: &mut Buffer, sprite: &[R], x: u16, y: u16) {
    let height = sprite.len();
    let mut logical_row = 0usize;
    while logical_row < height {
        let line = sprite[logical_row].as_ref();
        for (col, character) in line.chars().enumerate() {
            let top = tone_for(character);
            let bottom = if logical_row + 1 < height {
                tone_for(
                    sprite[logical_row + 1]
                        .as_ref()
                        .chars()
                        .nth(col)
                        .unwrap_or(' '),
                )
            } else {
                SemanticTone::Tone0
            };
            put_packed(
                area,
                buffer,
                x.saturating_add(u16::try_from(col).unwrap_or(u16::MAX)),
                y.saturating_add(u16::try_from(logical_row / 2).unwrap_or(u16::MAX)),
                top,
                bottom,
            );
        }
        logical_row += 2;
    }
}

/// Deterministic half-resolution downsample used by the Compact layout:
/// keeps every second logical row and column (12x10 -> 6x5), still packed at
/// two logical pixels per terminal row.
#[must_use]
pub fn downsample2x(sprite: &[&str]) -> Vec<String> {
    sprite
        .iter()
        .step_by(2)
        .map(|row| row.chars().step_by(2).collect::<String>())
        .collect()
}

/// Writes one terminal cell from two stacked logical pixels.
fn put_packed(
    area: Rect,
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    top: SemanticTone,
    bottom: SemanticTone,
) {
    if x >= area.width || y >= area.height {
        return;
    }
    let Some(cell) = buffer.cell_mut(Position {
        x: area.x + x,
        y: area.y + y,
    }) else {
        return;
    };
    let (symbol, style) = packed_cell(top, bottom);
    cell.set_symbol(symbol).set_style(style);
}

/// Resolves the half-block glyph and style for two stacked logical pixels.
fn packed_cell(top: SemanticTone, bottom: SemanticTone) -> (&'static str, Style) {
    let top_style = auto_style(top);
    let bottom_style = auto_style(bottom);
    match (top, bottom) {
        (SemanticTone::Tone0, SemanticTone::Tone0) => (" ", top_style),
        (SemanticTone::Tone0, _) => ("▄", bottom_style),
        (_, SemanticTone::Tone0) => ("▀", top_style),
        (top, bottom) if top == bottom => ("█", top_style),
        // Mixed mid tones: upper pixel as foreground, lower pixel as
        // background. Tone3 as a background degrades to the default
        // background (Auto cannot name the default foreground color).
        (_, _) => {
            let style = top_style.bg(bottom_style.fg.unwrap_or_default());
            ("▀", style)
        }
    }
}

/// The Full pet sprite for a pose: 10 logical columns x 12 logical rows,
/// packed into 6 terminal rows. `VISUAL_FIDELITY_UNVERIFIED`: simple
/// deterministic silhouettes, easily replaceable by the later vision pass.
#[must_use]
pub fn pet_sprite(pose: PetPose) -> &'static [&'static str] {
    match pose {
        PetPose::Idle => &PET_IDLE,
        PetPose::Blink => &PET_BLINK,
        PetPose::WalkA => &PET_WALK_A,
        PetPose::WalkB => &PET_WALK_B,
        PetPose::Sit => &PET_SIT,
        PetPose::Doze | PetPose::Sleep => &PET_SLEEP,
        PetPose::Yawn => &PET_YAWN,
        PetPose::Curious => &PET_CURIOUS,
        PetPose::Happy => &PET_HAPPY,
        PetPose::Upset => &PET_UPSET,
        PetPose::Eating => &PET_EATING,
        PetPose::Petted => &PET_PETTED,
    }
}

const PET_IDLE: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_BLINK: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_WALK_A: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    " .#.  .#. ",
    " ..   ..  ",
    "  .    .  ",
];

const PET_WALK_B: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    " .#.  .#. ",
    "   .. ..  ",
    "  .    .  ",
];

const PET_SIT: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    "  .####.  ",
    "   .##.   ",
    "   ....   ",
];

const PET_SLEEP: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    " .#....#. ",
    "  .####.  ",
    " .######. ",
    " .######. ",
    "  ......  ",
];

const PET_YAWN: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    " .#.  .#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_CURIOUS: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .####.#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_HAPPY: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    " .######. ",
    "  .####.  ",
    "  ....... ",
];

const PET_UPSET: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#.  .#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_EATING: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.##.#. ",
    " .#.##.#. ",
    " .#.oo.#. ",
    " .#.##.#. ",
    "  .####.  ",
    "  .#..#.  ",
    "  .#..#.  ",
    "  ..  ..  ",
];

const PET_PETTED: [&str; 12] = [
    "  ..  ..  ",
    " .##..##. ",
    " .#....#. ",
    " .#....#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#.oo.#. ",
    " .#....#. ",
    "  .####.  ",
    " .######. ",
    "  .####.  ",
    "  ....... ",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pose_sprite_has_consistent_row_widths() {
        for pose in [
            PetPose::Idle,
            PetPose::Blink,
            PetPose::WalkA,
            PetPose::WalkB,
            PetPose::Sit,
            PetPose::Doze,
            PetPose::Yawn,
            PetPose::Curious,
            PetPose::Happy,
            PetPose::Upset,
            PetPose::Eating,
            PetPose::Petted,
            PetPose::Sleep,
        ] {
            let sprite = pet_sprite(pose);
            let width = sprite[0].chars().count();
            for (index, row) in sprite.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    width,
                    "{pose:?} sprite row {index} has a different width"
                );
            }
            assert_eq!(sprite.len() % 2, 0, "{pose:?} must pack into whole rows");
        }
    }

    #[test]
    fn half_block_packing_uses_two_logical_pixels_per_row() {
        // A 2x2 sprite (dark top, background bottom) must render one terminal
        // row with the upper-half glyph.
        let sprite: [&str; 2] = ["##", ".."];
        let area = Rect::new(0, 0, 4, 4);
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        draw_sprite(area, &mut buffer, &sprite, 0, 0);
        assert_eq!(buffer.cell((0, 0)).expect("cell").symbol(), "▀");
        assert_eq!(buffer.cell((1, 0)).expect("cell").symbol(), "▀");
    }

    #[test]
    fn downsample_keeps_every_second_pixel() {
        let sprite = ["##########", ".........."];
        let compact = downsample2x(&sprite);
        assert_eq!(compact.len(), 1);
        assert_eq!(compact[0], "#####");
    }
}

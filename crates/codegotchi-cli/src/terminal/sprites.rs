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
use super::theme::{ResolvedPalette, SemanticTone, TerminalThemePreset};

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
#[allow(dead_code)]
pub fn draw_sprite<R: AsRef<str>>(area: Rect, buffer: &mut Buffer, sprite: &[R], x: u16, y: u16) {
    draw_sprite_with_palette(
        area,
        buffer,
        sprite,
        x,
        y,
        TerminalThemePreset::Auto.resolve(),
    );
}

/// Draws a logical-pixel sprite using the caller's resolved semantic palette.
pub fn draw_sprite_with_palette<R: AsRef<str>>(
    area: Rect,
    buffer: &mut Buffer,
    sprite: &[R],
    x: u16,
    y: u16,
    palette: ResolvedPalette,
) {
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
                palette,
            );
        }
        logical_row += 2;
    }
}

/// Writes one terminal cell from two stacked logical pixels.
fn put_packed(
    area: Rect,
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    top: SemanticTone,
    bottom: SemanticTone,
    palette: ResolvedPalette,
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
    let (symbol, style) = packed_cell_with_palette(top, bottom, palette);
    cell.set_symbol(symbol).set_style(style);
}

/// Resolves the half-block glyph and style using one concrete room palette.
fn packed_cell_with_palette(
    top: SemanticTone,
    bottom: SemanticTone,
    palette: ResolvedPalette,
) -> (&'static str, Style) {
    let top_style = palette.cell_style(top);
    let bottom_style = palette.cell_style(bottom);
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

/// The Full pet sprite for a pose: 10 logical columns x 10 logical rows,
/// packed into 5 terminal rows. The art is the round, blocky CodeGotchi
/// silhouette (big head, two square eyes, small mouth, alternating feet);
/// `VISUAL_FIDELITY_UNVERIFIED`: it is authored as logical-pixel grids and
/// packed with half-blocks, and still needs the later vision pass.
#[must_use]
pub fn pet_sprite(pose: PetPose) -> &'static [&'static str] {
    match pose {
        PetPose::Idle => &PET_IDLE,
        PetPose::Blink => &PET_BLINK,
        PetPose::WalkA => &PET_WALK_A,
        PetPose::WalkB => &PET_WALK_B,
        PetPose::Sit => &PET_SIT,
        PetPose::Doze => &PET_DOZE,
        PetPose::Sleep => &PET_SLEEP,
        PetPose::Yawn => &PET_YAWN,
        PetPose::Curious => &PET_CURIOUS,
        PetPose::Happy => &PET_HAPPY,
        PetPose::Upset => &PET_UPSET,
        PetPose::Eating => &PET_EATING,
        PetPose::Petted => &PET_PETTED,
    }
}

/// The Compact pet sprite for a pose: 7 logical columns x 6 logical rows,
/// packed into 3 terminal rows (the 2x logical downsample of the Full art).
#[must_use]
pub fn pet_sprite_compact(pose: PetPose) -> &'static [&'static str] {
    match pose {
        PetPose::Idle => &PET_IDLE_C,
        PetPose::Blink => &PET_BLINK_C,
        PetPose::WalkA => &PET_WALK_A_C,
        PetPose::WalkB => &PET_WALK_B_C,
        PetPose::Sit => &PET_SIT_C,
        PetPose::Doze => &PET_DOZE_C,
        PetPose::Sleep => &PET_SLEEP_C,
        PetPose::Yawn => &PET_YAWN_C,
        PetPose::Curious => &PET_CURIOUS_C,
        PetPose::Happy => &PET_HAPPY_C,
        PetPose::Upset => &PET_UPSET_C,
        PetPose::Eating => &PET_EATING_C,
        PetPose::Petted => &PET_PETTED_C,
    }
}

// Full sprites: 10 logical rows (5 terminal rows) x 10 logical columns.
// Each terminal row packs two logical pixels: '#' foreground on both halves
// renders as '█', top-only as '▀', bottom-only as '▄', and ' ' (Tone0)
// renders as the room background.

const PET_IDLE: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_BLINK: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #       #",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_WALK_A: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    " #  ##  # ",
    "          ",
];

const PET_WALK_B: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "#  ##  #  ",
    "          ",
];

const PET_SIT: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ####    ",
    "  ####    ",
];

const PET_DOZE: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #       #",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_SLEEP: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #       #",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_YAWN: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " ###  ### ",
    " #  ##  # ",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_CURIOUS: [&str; 10] = PET_IDLE;

const PET_HAPPY: [&str; 10] = [
    "          ",
    " ######## ",
    " #  ## ## ",
    " #  ## ## ",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_UPSET: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #      # ",
    " #  ##  # ",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_EATING: [&str; 10] = [
    "          ",
    "  ######  ",
    " #  ## ## ",
    " #  ## ## ",
    " #      # ",
    " # #### # ",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

const PET_PETTED: [&str; 10] = [
    "          ",
    " ######## ",
    " #  ## ## ",
    " #       #",
    " #       #",
    " #  ##   #",
    " #      # ",
    " ######## ",
    "  ######  ",
    "          ",
];

// Compact sprites: 6 logical rows (3 terminal rows) x 7 logical columns.

const PET_IDLE_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", " ##### ", "       ",
];

const PET_BLINK_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "#     #", " ##### ", "       ",
];

const PET_WALK_A_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", "# ##  #", "       ",
];

const PET_WALK_B_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", "  ## ##", "       ",
];

const PET_SIT_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", "  ###  ", "  ###  ",
];

const PET_DOZE_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "#     #", " ##### ", "       ",
];

const PET_SLEEP_C: [&str; 6] = [
    "       ", " ##### ", "#     #", "#     #", " ##### ", "       ",
];

const PET_YAWN_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", " ##  ##", "   ##  ",
];

const PET_CURIOUS_C: [&str; 6] = PET_IDLE_C;

const PET_HAPPY_C: [&str; 6] = [
    "       ", "#######", "# ## ##", "# ## ##", " ##### ", "       ",
];

const PET_UPSET_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", "###### ", "       ",
];

const PET_EATING_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", "#     #", "# ### #",
];

const PET_PETTED_C: [&str; 6] = [
    "       ", "#######", "# ## ##", "#     #", " ##### ", "       ",
];

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_POSES: [PetPose; 13] = [
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
    ];

    #[test]
    fn every_pose_sprite_has_consistent_row_widths() {
        for pose in ALL_POSES {
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
    fn every_compact_pose_sprite_is_consistent_and_smaller_than_full() {
        for pose in ALL_POSES {
            let sprite = pet_sprite_compact(pose);
            let width = sprite[0].chars().count();
            for (index, row) in sprite.iter().enumerate() {
                assert_eq!(
                    row.chars().count(),
                    width,
                    "compact {pose:?} sprite row {index} has a different width"
                );
            }
            assert_eq!(
                sprite.len() % 2,
                0,
                "compact {pose:?} must pack into whole rows"
            );
            assert!(
                width < pet_sprite(pose)[0].chars().count(),
                "compact {pose:?} should be narrower than the Full sprite"
            );
        }
    }

    /// The packed Full idle sprite must reproduce the classic round blob
    /// glyphs so the 'cuter' art style survives the logical-pixel pipeline.
    #[test]
    fn full_idle_packs_to_the_classic_blob_glyphs() {
        let area = Rect::new(0, 0, 10, 5);
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        draw_sprite(area, &mut buffer, pet_sprite(PetPose::Idle), 0, 0);
        let packed: Vec<String> = (0..5)
            .map(|row| {
                (0..10)
                    .map(|col| buffer.cell((col, row)).expect("cell").symbol().to_owned())
                    .collect()
            })
            .collect();
        assert_eq!(
            packed,
            [
                "  ▄▄▄▄▄▄  ",
                " █  ██ ██ ",
                " █  ▄▄   █",
                " █▄▄▄▄▄▄█ ",
                "  ▀▀▀▀▀▀  ",
            ]
        );
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
}

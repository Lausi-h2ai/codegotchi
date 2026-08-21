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
            let logical_x = area
                .x
                .saturating_add(x)
                .saturating_add(u16::try_from(col).unwrap_or(u16::MAX));
            let logical_y = area
                .y
                .saturating_add(y)
                .saturating_add(u16::try_from(logical_row).unwrap_or(u16::MAX));
            let top = palette.sample_logical_tone(tone_for(character), logical_x, logical_y);
            let bottom = if logical_row + 1 < height {
                let bottom_tone = tone_for(
                    sprite[logical_row + 1]
                        .as_ref()
                        .chars()
                        .nth(col)
                        .unwrap_or(' '),
                );
                palette.sample_logical_tone(bottom_tone, logical_x, logical_y.saturating_add(1))
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

/// The Full pet sprite for a pose: 18 logical columns x 14 logical rows,
/// packed into 7 terminal rows. Every pose shares this canvas so the pet
/// remains a large, stable focal target while its expression changes.
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

// Full sprites: 14 logical rows (7 terminal rows) x 18 logical columns.

const PET_IDLE: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "                  ",
];

const PET_BLINK: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  ..  ..  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "                  ",
];

const PET_WALK_A: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "   ####    ####   ",
    "      ##  ##      ",
];

const PET_WALK_B: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "   ####    ####   ",
    "    ##  ##        ",
];

const PET_SIT: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "   ############   ",
    "   ############   ",
];

const PET_DOZE: [&str; 14] = [
    "                  ",
    "                  ",
    "       ##         ",
    "     ######       ",
    "   ##  oo  ###    ",
    "  ##   ..   ####  ",
    " ##    ##     ### ",
    "##################",
    " #################",
    "  ############### ",
    "   #############  ",
    "     #########    ",
    "       ######     ",
    "                  ",
];

const PET_SLEEP: [&str; 14] = [
    "                  ",
    "      ##    ##    ",
    "    ##########    ",
    "   ############   ",
    "  ##  ..  ..  ##  ",
    "  ##   ####   ##  ",
    "##################",
    "##  oooooooo  ##  ",
    "##  ##########  ##",
    "##  ##############",
    " ################ ",
    "  ##############  ",
    "                  ",
    "                  ",
];

const PET_YAWN: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####..oo..##",
    "##...######.....##",
    " ##..###oo###..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "                  ",
];

const PET_CURIOUS: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo   oo ##   ",
    " ##  ..   .. ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "                  ",
];

const PET_HAPPY: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....######....##",
    "##...########...##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "        ..        ",
];

const PET_UPSET: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "      ..  ..      ",
];

const PET_EATING: [&str; 14] = [
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##############  ",
    " ##  oo  oo  ##   ",
    " ##  ..  ..  ##   ",
    "##......##......##",
    "##....####......##",
    "##...######.....##",
    " ##..########..## ",
    "  ##.####..##..## ",
    "  ##  ####  ##  ##",
    "    ##  ##      ##",
    "      ####        ",
];

const PET_PETTED: [&str; 14] = [
    "        ..        ",
    "      ##    ##    ",
    "     ####  ####   ",
    "   ############   ",
    "  ##  oo  oo  ##  ",
    "  ##  ..  ..  ##  ",
    "##......##......##",
    "##....######....##",
    "##...########...##",
    " ##..########..## ",
    "  ##.########.##  ",
    "  ##############  ",
    "    ####  ####    ",
    "                  ",
];

// Compact sprites: 6 logical rows (3 terminal rows) x 7 logical columns.

const PET_IDLE_C: [&str; 6] = [
    "  # #  ", " ##### ", "# oo ##", "# ..  #", " ##### ", "  # #  ",
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
    "       ", "       ", " . .   ", "###    ", "## o###", "#######",
];

const PET_SLEEP_C: [&str; 6] = [
    "       ", " ## ## ", "#######", "## ..##", "#######", " ##### ",
];

const PET_YAWN_C: [&str; 6] = [
    "       ", " ##### ", "# ## ##", "# ## ##", " ##  ##", "   ##  ",
];

const PET_CURIOUS_C: [&str; 6] = [
    "  # #  ", " ##### ", "# oo ##", "#  . .#", " ##### ", "  # #  ",
];

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

    /// Full art uses a shared, tall canvas so every pose keeps the same
    /// hitbox and focal scale while its face/body details change in place.
    #[test]
    fn every_full_pose_uses_the_shared_tall_canvas() {
        let width = pet_sprite(PetPose::Idle)[0].chars().count();
        assert!(width >= 16, "Full mascot needs a round focal width");
        for pose in ALL_POSES {
            let sprite = pet_sprite(pose);
            assert_eq!(
                sprite.len(),
                14,
                "{pose:?} should occupy seven terminal rows"
            );
            assert_eq!(
                sprite[0].chars().count(),
                width,
                "{pose:?} should share the Full canvas width"
            );
        }
    }

    #[test]
    fn full_poses_change_their_load_bearing_face_or_body_details() {
        for (index, first) in ALL_POSES.iter().enumerate() {
            for second in ALL_POSES.iter().skip(index + 1) {
                assert_ne!(
                    pet_sprite(*first),
                    pet_sprite(*second),
                    "{first:?} and {second:?} must not share one Full pose"
                );
            }
        }
    }

    #[test]
    fn compact_yawn_and_curious_poses_are_not_idle_copies() {
        assert_ne!(
            pet_sprite_compact(PetPose::Yawn),
            pet_sprite_compact(PetPose::Idle)
        );
        assert_ne!(
            pet_sprite_compact(PetPose::Curious),
            pet_sprite_compact(PetPose::Idle)
        );
    }

    #[test]
    fn full_idle_packs_with_a_round_body_and_face_details() {
        let area = Rect::new(0, 0, 18, 7);
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        draw_sprite(area, &mut buffer, pet_sprite(PetPose::Idle), 0, 0);
        let packed: Vec<String> = (0..7)
            .map(|row| {
                (0..18)
                    .map(|col| buffer.cell((col, row)).expect("cell").symbol().to_owned())
                    .collect()
            })
            .collect();
        assert_eq!(
            packed,
            vec![
                "     ▄██▄  ▄██▄   ".to_owned(),
                "  ▄████████████▄  ".to_owned(),
                " ██  █▀  █▀  ██   ".to_owned(),
                "██▀ ▀ █▄██▀ ▀ ▀ ██".to_owned(),
                "▀██ ▀██████▄█▄▀▄█▀".to_owned(),
                "  ███████████▄██  ".to_owned(),
                "    ▀▀▀▀  ▀▀▀▀    ".to_owned(),
            ],
            "Full idle packed output changed without updating the visual contract"
        );
    }

    #[test]
    fn half_block_packing_uses_two_logical_pixels_per_row() {
        // A 2x2 sprite (foreground top, background bottom) must render one
        // terminal row with the upper-half glyph.
        let sprite: [&str; 2] = ["##", ".."];
        let area = Rect::new(0, 0, 4, 4);
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        draw_sprite(area, &mut buffer, &sprite, 0, 0);
        let symbols: Vec<_> = (0..2)
            .map(|x| buffer.cell((x, 0)).expect("cell").symbol().to_owned())
            .collect();
        assert!(
            symbols.iter().all(|symbol| symbol == "▀" || symbol == "█"),
            "ordered dithering may only replace a foreground pixel with the background, got {symbols:?}"
        );
    }

    #[test]
    fn auto_sprite_sampling_uses_each_nonzero_origin_as_room_coordinates() {
        let area = Rect::new(7, 11, 32, 20);
        let sprite = ["...", "...", "...", "..."];
        let palette = TerminalThemePreset::Auto.resolve();

        for &(origin_x, origin_y) in &[(2, 3), (11, 8)] {
            let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
            draw_sprite_with_palette(area, &mut buffer, &sprite, origin_x, origin_y, palette);

            for logical_row in (0..sprite.len()).step_by(2) {
                for col in 0..sprite[logical_row].chars().count() {
                    let logical_x = area
                        .x
                        .saturating_add(origin_x)
                        .saturating_add(u16::try_from(col).unwrap());
                    let logical_y = area
                        .y
                        .saturating_add(origin_y)
                        .saturating_add(u16::try_from(logical_row).unwrap());
                    let top =
                        palette.sample_logical_tone(SemanticTone::Tone1, logical_x, logical_y);
                    let bottom = palette.sample_logical_tone(
                        SemanticTone::Tone1,
                        logical_x,
                        logical_y.saturating_add(1),
                    );
                    let expected = packed_cell_with_palette(top, bottom, palette).0;
                    let cell = buffer
                        .cell((logical_x, area.y + origin_y + logical_row as u16 / 2))
                        .expect("sprite cell exists");
                    assert_eq!(
                        cell.symbol(),
                        expected,
                        "sprite origin ({origin_x}, {origin_y}) at logical ({col}, {logical_row})"
                    );
                }
            }
        }
    }

    #[test]
    fn auto_sprite_sampling_continues_across_adjacent_layers() {
        let area = Rect::new(9, 13, 20, 8);
        let sprite = ["...", "..."];
        let palette = TerminalThemePreset::Auto.resolve();
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        draw_sprite_with_palette(area, &mut buffer, &sprite, 2, 1, palette);
        draw_sprite_with_palette(area, &mut buffer, &sprite, 5, 1, palette);

        for origin_x in [2, 5] {
            for col in 0..sprite[0].chars().count() {
                let logical_x = area
                    .x
                    .saturating_add(origin_x)
                    .saturating_add(u16::try_from(col).unwrap());
                let logical_y = area.y + 1;
                let top = palette.sample_logical_tone(SemanticTone::Tone1, logical_x, logical_y);
                let bottom =
                    palette.sample_logical_tone(SemanticTone::Tone1, logical_x, logical_y + 1);
                let expected = packed_cell_with_palette(top, bottom, palette).0;
                let cell = buffer
                    .cell((logical_x, area.y + 1))
                    .expect("sprite cell exists");
                assert_eq!(
                    cell.symbol(),
                    expected,
                    "adjacent sprite at x {origin_x}, column {col}"
                );
            }
        }
    }

    #[test]
    fn floor_doze_and_bed_sleep_use_distinct_sprite_grids() {
        assert_ne!(
            pet_sprite(PetPose::Doze),
            pet_sprite(PetPose::Sleep),
            "floor doze and bed sleep need distinct visual contexts"
        );
        assert_ne!(
            pet_sprite_compact(PetPose::Doze),
            pet_sprite_compact(PetPose::Sleep),
            "Compact floor doze and bed sleep need distinct visual contexts"
        );
    }

    #[test]
    fn floor_doze_uses_a_horizontal_curl_pose() {
        assert_eq!(
            pet_sprite(PetPose::Doze),
            &[
                "                  ",
                "                  ",
                "       ##         ",
                "     ######       ",
                "   ##  oo  ###    ",
                "  ##   ..   ####  ",
                " ##    ##     ### ",
                "##################",
                " #################",
                "  ############### ",
                "   #############  ",
                "     #########    ",
                "       ######     ",
                "                  ",
            ],
            "floor doze should be a horizontal curled pose"
        );
    }

    #[test]
    fn sleep_states_keep_face_and_blanket_detail() {
        let doze = pet_sprite(PetPose::Doze).concat();
        let sleep = pet_sprite(PetPose::Sleep).concat();
        assert!(
            doze.contains('o'),
            "floor doze should retain a mid-tone curled-face detail"
        );
        assert!(
            sleep.contains('.'),
            "bed sleep should retain a dark-tone blanket/face detail"
        );
    }

    #[test]
    fn idle_sprite_keeps_ears_eyes_and_feet_details() {
        let idle = pet_sprite(PetPose::Idle).concat();
        assert!(idle.contains('o'), "idle face should contain mid-tone eyes");
        assert!(
            idle.contains('.'),
            "idle silhouette should contain dark details"
        );
    }
}

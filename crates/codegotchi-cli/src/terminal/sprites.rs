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

/// The Compact pet sprite for a pose: 12 logical columns x 10 logical rows,
/// packed into a complete five-terminal-row mascot.
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
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ###### # ",
    "   ###############",
    "  ############# ##",
    "  ###  ##  #######",
    "  ###  ##  #### ##",
    "  #####   ##### ##",
    "   ############ ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_BLINK: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ###### # ",
    "   ###############",
    "  ############# ##",
    "  ####  ##  ######",
    "  ############# ##",
    "  #####   ##### ##",
    "   ############ ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_WALK_A: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ###### # ",
    "   ###############",
    "  ############# ##",
    "  ###  ##  #######",
    "  ###  ##  #### ##",
    "  #####   ##### ##",
    "   ############ ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "  ####     ####   ",
    "   ####     ####  ",
];

const PET_WALK_B: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  #########",
    "   ############## ",
    "  ################",
    "  ###  ##  ###### ",
    "  ###  ##  ###### ",
    "  #####   ####### ",
    "   ############## ",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####     ####  ",
    "  ####     ####   ",
];

const PET_SIT: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ######   ",
    "   ############   ",
    "  #############   ",
    "  ###  ##  ####   ",
    "  ###  ##  ####   ",
    " ### ##########   ",
    "  ### #########   ",
    "  #####oooo###    ",
    "  ######  #####   ",
    "    ##########    ",
    "    ####  ####    ",
    "    ####  ####    ",
];

const PET_DOZE: [&str; 14] = [
    "                  ",
    "                  ",
    "                  ",
    "              ### ",
    "            ##### ",
    "   ########### ## ",
    " ################ ",
    "################# ",
    "##################",
    "##  #####  #######",
    " ######oooo###### ",
    "  #### ####### #  ",
    "   ############   ",
    "                  ",
];

const PET_SLEEP: [&str; 14] = [
    "                  ",
    "                  ",
    "       ####       ",
    "     ########     ",
    "    ##########    ",
    "    ##  ##  ##    ",
    "    ##########    ",
    "     ###  ####    ",
    "      ##ooo##     ",
    "      ##ooo##     ",
    "       #####      ",
    "                  ",
    "                  ",
    "                  ",
];

const PET_YAWN: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ###### # ",
    "   ###############",
    "  ############# ##",
    "  ###  ##  #######",
    "  ############# ##",
    "  ####      ### ##",
    "   ####    #### ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_CURIOUS: [&str; 14] = [
    " #          #     ",
    " ####     #####   ",
    " ######  ###### ##",
    "   ###############",
    "  ############# ##",
    "  ###  ##  #######",
    "  ###  ##  #######",
    "  #####   ##### ##",
    "   ############ ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_HAPPY: [&str; 14] = [
    "   #        #     ",
    "  ###     #####  #",
    "  #####  ###### ##",
    "   ###############",
    "  ############# ##",
    "  ###  ##  #######",
    "  #### ## ##### ##",
    "  ####      ### ##",
    "   ############ ##",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_UPSET: [&str; 14] = [
    "                  ",
    "   ####    ####   ",
    "  ##############  ",
    "  ##############  ",
    " ###############  ",
    " ####  ##  #####  ",
    " ##### ## ######  ",
    " #######  ######  ",
    " ######    ####   ",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "    ####  ####    ",
    "    ####  ####    ",
];

const PET_EATING: [&str; 14] = [
    "      #           ",
    "     ###       #  ",
    "    #####     ### ",
    "   ############## ",
    "  ############## #",
    "  ####  ##########",
    "  ####  ####### ##",
    "  ############# ##",
    "   ####   ##### # ",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

const PET_PETTED: [&str; 14] = [
    "   #        #     ",
    "  ###     #####   ",
    "  #####  ###### # ",
    "    ############# ",
    "   ############ ##",
    "   ##  ###  ######",
    "   ############## ",
    "   ###      ##### ",
    "   ############## ",
    "   ####oooo####   ",
    "   ####oooo####   ",
    "    ##########    ",
    "   ####    ####   ",
    "   ####    ####   ",
];

// Compact sprites: 10 logical rows (5 terminal rows) x 12 logical columns.

const PET_IDLE_C: [&str; 10] = [
    "  #    #    ",
    " ###  ###   ",
    " ########## ",
    "############",
    "##  #  #  ##",
    "##  ##   ###",
    "##  oooo  ##",
    "## oooooo ##",
    "##  #### ###",
    "##  #### ###",
];

const PET_BLINK_C: [&str; 10] = [
    "  #    #    ",
    " ###  ###   ",
    " ########## ",
    "############",
    "##  ##  ##  ",
    "##  ##   ###",
    "##  oooo  ##",
    "## oooooo ##",
    "##  #### ###",
    "##  #### ###",
];

const PET_WALK_A_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "### #### ###",
    "##  #  #  ##",
    "##  ##    ##",
    "##  oooo  ##",
    "## oooooo ##",
    "##  ####  ##",
    "###  ### ###",
];

const PET_WALK_B_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "############",
    "## #  #   ##",
    "##    #   ##",
    "##  oooo  ##",
    "## oooooo ##",
    "## ####  ###",
    "##  ### ####",
];

const PET_SIT_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "### #### ###",
    "##  #  #  ##",
    "##    #   ##",
    "## oooooo ##",
    "## oooooo ##",
    "###  ##### #",
    "###  #######",
];

const PET_DOZE_C: [&str; 10] = [
    "            ",
    "     ###    ",
    "   #######  ",
    " ##  ##  ###",
    "############",
    "##  ####  ##",
    "############",
    "##  oooo  ##",
    " ## oooo ###",
    "   #######  ",
];

const PET_SLEEP_C: [&str; 10] = [
    "            ",
    "    ###     ",
    "  #######   ",
    " ##  ##  ## ",
    "############",
    "##  ##  ####",
    "############",
    " ## ooo  ###",
    "  ## oooo###",
    "    ######  ",
];

const PET_YAWN_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "### #### ###",
    "##  #  #  ##",
    "##     #####",
    "##  oooo  ##",
    "## oooooo ##",
    "### #### ###",
    "###  ### ###",
];

const PET_CURIOUS_C: [&str; 10] = [
    "  #     ### ",
    " ###   #### ",
    " ########## ",
    "### #### ###",
    "##  #  # ###",
    "##    #   ##",
    "##  oooo  ##",
    "## oooooo ##",
    "##  ####  ##",
    "  ###  #####",
];

const PET_HAPPY_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "### #### ###",
    "##  # #   ##",
    "##    #   ##",
    "## oooooo ##",
    "## oooooo ##",
    "###  ### ###",
    "### #### ###",
];

const PET_UPSET_C: [&str; 10] = [
    "  ##    ##  ",
    " ####  #### ",
    " ########## ",
    "############",
    "##  ##  ##  ",
    "## ##   # ##",
    "##  ####  ##",
    "##  ####  ##",
    "##  ####  ##",
    "###  #######",
];

const PET_EATING_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "############",
    "##  #  #### ",
    "##  ####  ##",
    "##  oooo  ##",
    "## oooooo ##",
    "##  ####  ##",
    "##  #### ###",
];

const PET_PETTED_C: [&str; 10] = [
    "   #    #   ",
    "  ###  ###  ",
    " ########## ",
    "### #### ###",
    "##  # #  ###",
    "##   #    ##",
    "## oooooo ##",
    "## oooooo ##",
    "##  ##  ####",
    "##  #### ###",
];

// Minimal sprites: 6 logical rows (3 terminal rows) x 9 logical columns.

const PET_NEUTRAL_M: [&str; 6] = [
    " #   #   ",
    "#########",
    "## # # ##",
    "## ##  ##",
    "## ooo ##",
    "###   ###",
];

const PET_CLOSED_EYES_M: [&str; 6] = [
    " #   #   ",
    "#########",
    "# ## ## #",
    "## ##  ##",
    "## ooo ##",
    "###   ###",
];

const PET_POSITIVE_M: [&str; 6] = [
    " #   #   ",
    "#########",
    "## # # ##",
    "## ### ##",
    "##  o  ##",
    "###   ###",
];

const PET_NEGATIVE_M: [&str; 6] = [
    " #   #   ",
    "#########",
    "## # # ##",
    "## # # ##",
    "## ### ##",
    "###   ###",
];

const PET_YAWN_M: [&str; 6] = [
    " #   #   ",
    "#########",
    "## # # ##",
    "## ### ##",
    "## ooo ##",
    "###   ###",
];

const PET_EATING_M: [&str; 6] = [
    " #    #  ",
    "#########",
    "## #  ###",
    "## ### ##",
    "## ooo ##",
    "###   ###",
];

/// The Minimal pet sprite keeps a complete silhouette in the three-row care
/// strip instead of cropping the Compact mascot's upper body.
#[must_use]
pub fn pet_sprite_minimal(pose: PetPose) -> &'static [&'static str; 6] {
    match pose {
        PetPose::Blink | PetPose::Doze | PetPose::Sleep => &PET_CLOSED_EYES_M,
        PetPose::Happy | PetPose::Petted => &PET_POSITIVE_M,
        PetPose::Upset => &PET_NEGATIVE_M,
        PetPose::Yawn => &PET_YAWN_M,
        PetPose::Eating => &PET_EATING_M,
        PetPose::Idle | PetPose::WalkA | PetPose::WalkB | PetPose::Sit | PetPose::Curious => {
            &PET_NEUTRAL_M
        }
    }
}

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

    fn assert_sprite_shape(label: &str, sprite: &[&str], width: usize, height: usize) {
        assert_eq!(
            sprite.len(),
            height,
            "{label} must have {height} logical rows"
        );
        assert_eq!(height % 2, 0, "{label} must pack into whole terminal rows");

        let mut occupied = 0;
        let mut tone2 = 0;
        for (row_index, row) in sprite.iter().enumerate() {
            assert_eq!(
                row.chars().count(),
                width,
                "{label} row {row_index} must have {width} logical columns"
            );
            for character in row.chars() {
                assert!(
                    matches!(character, ' ' | 'o' | '#'),
                    "{label} contains undeclared tone {character:?}"
                );
                if character != ' ' {
                    occupied += 1;
                }
                if character == 'o' {
                    tone2 += 1;
                }
            }
        }
        assert!(occupied > 0, "{label} must contain an occupied mascot");
        assert!(
            tone2 * 5 <= occupied,
            "{label} must keep Tone2 below one fifth of occupied cells"
        );
    }

    fn occupied_bounds(sprite: &[&str]) -> (usize, usize, usize, usize) {
        let mut bounds: Option<(usize, usize, usize, usize)> = None;
        for (y, row) in sprite.iter().enumerate() {
            for (x, character) in row.chars().enumerate() {
                if character != ' ' {
                    bounds = Some(match bounds {
                        Some((min_x, min_y, max_x, max_y)) => {
                            (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                        }
                        None => (x, y, x, y),
                    });
                }
            }
        }
        bounds.expect("sprite must contain an occupied pixel")
    }

    fn occupied_row_bounds(sprite: &[&str]) -> Vec<Option<(usize, usize)>> {
        sprite
            .iter()
            .map(|row| {
                let mut bounds: Option<(usize, usize)> = None;
                for (x, character) in row.chars().enumerate() {
                    if character != ' ' {
                        bounds = Some(match bounds {
                            Some((min_x, max_x)) => (min_x.min(x), max_x.max(x)),
                            None => (x, x),
                        });
                    }
                }
                bounds
            })
            .collect()
    }

    fn occupied_runs(row: &str) -> Vec<(usize, usize)> {
        let mut runs = Vec::new();
        let mut start = None;
        for (x, character) in row.chars().enumerate() {
            if character != ' ' {
                start.get_or_insert(x);
            } else if let Some(run_start) = start.take() {
                runs.push((run_start, x - 1));
            }
        }
        if let Some(run_start) = start {
            runs.push((run_start, row.chars().count() - 1));
        }
        runs
    }

    fn interior_space_runs(row: &str) -> Vec<(usize, usize)> {
        let occupied = occupied_runs(row);
        let Some((min_x, _)) = occupied.first().copied() else {
            return Vec::new();
        };
        let Some((_, max_x)) = occupied.last().copied() else {
            return Vec::new();
        };
        let mut runs = Vec::new();
        let mut start = None;
        for (x, character) in row.chars().enumerate().skip(min_x).take(max_x - min_x + 1) {
            if character == ' ' {
                start.get_or_insert(x);
            } else if let Some(run_start) = start.take() {
                runs.push((run_start, x - run_start));
            }
        }
        if let Some(run_start) = start {
            runs.push((run_start, max_x + 1 - run_start));
        }
        runs
    }

    fn face_space_runs(sprite: &[&str]) -> Vec<Vec<(usize, usize)>> {
        let (_, min_y, _, max_y) = occupied_bounds(sprite);
        let first_face_row = min_y.saturating_add(2);
        let last_face_row = max_y.saturating_sub(3);
        if first_face_row > last_face_row {
            return Vec::new();
        }
        (first_face_row..=last_face_row)
            .map(|y| interior_space_runs(sprite[y]))
            .collect()
    }

    fn has_two_wide_eye_gaps(sprite: &[&str]) -> bool {
        face_space_runs(sprite)
            .iter()
            .any(|runs| runs.iter().filter(|(_, width)| *width >= 2).count() >= 2)
    }

    fn has_mouth_gap(sprite: &[&str], minimum_width: usize) -> bool {
        face_space_runs(sprite)
            .iter()
            .any(|runs| runs.iter().any(|(_, width)| *width >= minimum_width))
    }

    fn has_two_pointed_ear_peaks(sprite: &[&str]) -> bool {
        let rows = occupied_row_bounds(sprite);
        let Some(top_y) = rows.iter().position(Option::is_some) else {
            return false;
        };
        let peaks = occupied_runs(sprite[top_y]);
        peaks.len() == 2
            && peaks.iter().all(|(start, end)| {
                end - start < 2
                    && (top_y + 1..=(top_y + 2).min(sprite.len() - 1)).all(|y| {
                        occupied_runs(sprite[y])
                            .iter()
                            .any(|(next_start, next_end)| {
                                *next_end >= start.saturating_sub(1)
                                    && *next_start <= end.saturating_add(1)
                            })
                    })
            })
    }

    fn has_high_raised_tail(sprite: &[&str]) -> bool {
        let (_, min_y, max_x, _) = occupied_bounds(sprite);
        occupied_row_bounds(sprite)
            .get(min_y.saturating_add(1))
            .and_then(|bounds| *bounds)
            .is_some_and(|(_, row_max)| row_max == max_x)
    }

    fn has_crescent_eyes(sprite: &[&str]) -> bool {
        face_space_runs(sprite)
            .iter()
            .take(5)
            .any(|runs| runs.iter().filter(|(_, width)| *width == 1).count() >= 2)
    }

    fn has_broad_smile(sprite: &[&str]) -> bool {
        face_space_runs(sprite)
            .iter()
            .skip(2)
            .any(|runs| runs.iter().any(|(_, width)| *width >= 4))
    }

    fn has_relaxed_shoulders(sprite: &[&str]) -> bool {
        let (_, min_y, _, _) = occupied_bounds(sprite);
        let rows = occupied_row_bounds(sprite);
        let Some(first) = rows.get(min_y.saturating_add(3)).and_then(|bounds| *bounds) else {
            return false;
        };
        let Some(second) = rows.get(min_y.saturating_add(4)).and_then(|bounds| *bounds) else {
            return false;
        };
        second.1 - second.0 >= first.1 - first.0 + 2
    }

    fn has_lowered_ears(sprite: &[&str]) -> bool {
        let rows = occupied_row_bounds(sprite);
        let Some(top_y) = rows.iter().position(Option::is_some) else {
            return false;
        };
        top_y > 0
            && occupied_runs(sprite[top_y])
                .iter()
                .all(|(start, end)| end - start + 1 >= 3)
    }

    fn has_downturned_mouth(sprite: &[&str]) -> bool {
        face_space_runs(sprite)
            .iter()
            .enumerate()
            .skip(5)
            .any(|(_, runs)| runs.iter().any(|(_, width)| *width == 4))
    }

    fn has_tucked_tail(sprite: &[&str]) -> bool {
        let (min_x, _, max_x, _) = occupied_bounds(sprite);
        max_x <= sprite[0].chars().count().saturating_sub(3) && min_x > 0
    }

    fn has_food_facing_asymmetry(sprite: &[&str]) -> bool {
        let rows = occupied_row_bounds(sprite);
        let Some(top_y) = rows.iter().position(Option::is_some) else {
            return false;
        };
        occupied_runs(sprite[top_y]).len() == 1
            && face_space_runs(sprite)
                .iter()
                .any(|runs| runs.len() == 1 && runs[0].1 >= 2)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ExpressionSignature {
        high_raised_tail: bool,
        crescent_eyes: bool,
        broad_smile: bool,
        relaxed_shoulders: bool,
        lowered_ears: bool,
        downturned_mouth: bool,
        tucked_tail: bool,
        food_facing_asymmetry: bool,
    }

    fn expression_signature(sprite: &[&str]) -> ExpressionSignature {
        ExpressionSignature {
            high_raised_tail: has_high_raised_tail(sprite),
            crescent_eyes: has_crescent_eyes(sprite),
            broad_smile: has_broad_smile(sprite),
            relaxed_shoulders: has_relaxed_shoulders(sprite),
            lowered_ears: has_lowered_ears(sprite),
            downturned_mouth: has_downturned_mouth(sprite),
            tucked_tail: has_tucked_tail(sprite),
            food_facing_asymmetry: has_food_facing_asymmetry(sprite),
        }
    }

    fn occupied_component_count(sprite: &[&str]) -> usize {
        let rows: Vec<Vec<char>> = sprite.iter().map(|row| row.chars().collect()).collect();
        let height = rows.len();
        let width = rows.first().map_or(0, Vec::len);
        let mut visited = vec![vec![false; width]; height];
        let mut components = 0;

        for y in 0..height {
            for x in 0..width {
                if rows[y][x] == ' ' || visited[y][x] {
                    continue;
                }
                components += 1;
                let mut pending = vec![(x, y)];
                visited[y][x] = true;
                while let Some((x, y)) = pending.pop() {
                    for (next_x, next_y) in [
                        (x.checked_sub(1), Some(y)),
                        (x.checked_add(1).filter(|next| *next < width), Some(y)),
                        (Some(x), y.checked_sub(1)),
                        (Some(x), y.checked_add(1).filter(|next| *next < height)),
                    ] {
                        let (Some(next_x), Some(next_y)) = (next_x, next_y) else {
                            continue;
                        };
                        if rows[next_y][next_x] != ' ' && !visited[next_y][next_x] {
                            visited[next_y][next_x] = true;
                            pending.push((next_x, next_y));
                        }
                    }
                }
            }
        }

        components
    }

    #[test]
    fn full_pose_grids_are_18_by_14_and_use_declared_tones() {
        for pose in ALL_POSES {
            let sprite = pet_sprite(pose);
            assert_sprite_shape(&format!("Full {pose:?}"), sprite, 18, 14);
            let (min_x, min_y, max_x, max_y) = occupied_bounds(sprite);
            assert!(max_x < 18 && max_y < 14 && min_x <= max_x && min_y <= max_y);
        }
    }

    #[test]
    fn full_pose_occupied_cells_are_one_four_way_component() {
        for pose in ALL_POSES {
            let sprite = pet_sprite(pose);
            assert_sprite_shape(&format!("Full {pose:?}"), sprite, 18, 14);
            assert_eq!(
                occupied_component_count(sprite),
                1,
                "{pose:?} must have one connected occupied component"
            );
        }
    }

    #[test]
    fn pose_specific_full_geometry_matches_the_room_contract() {
        assert_eq!(
            occupied_bounds(pet_sprite(PetPose::Idle)),
            occupied_bounds(pet_sprite(PetPose::Blink)),
            "idle and blink must share one occupied hitbox"
        );

        let (_, doze_min_y, doze_max_x, doze_max_y) = occupied_bounds(pet_sprite(PetPose::Doze));
        let (doze_min_x, _, _, _) = occupied_bounds(pet_sprite(PetPose::Doze));
        let doze_width = doze_max_x - doze_min_x + 1;
        let doze_packed_height = doze_max_y / 2 - doze_min_y / 2 + 1;
        assert!(
            doze_width > doze_packed_height,
            "floor doze should be wider than tall after half-block packing"
        );

        let (sleep_min_x, sleep_min_y, sleep_max_x, sleep_max_y) =
            occupied_bounds(pet_sprite(PetPose::Sleep));
        assert!(
            sleep_min_x <= sleep_max_x && sleep_max_x < 18,
            "bed sleep must fit the 18-column canvas"
        );
        assert!(
            sleep_min_y <= sleep_max_y && sleep_max_y < 14,
            "bed sleep must fit the 14-row canvas"
        );
    }

    #[test]
    fn responsive_pose_grids_have_declared_complete_canvases() {
        for pose in ALL_POSES {
            let compact = pet_sprite_compact(pose);
            assert_sprite_shape(&format!("Compact {pose:?}"), compact, 12, 10);
            assert!(
                compact[0].chars().count() < pet_sprite(pose)[0].chars().count(),
                "compact {pose:?} should be narrower than the Full sprite"
            );
            let minimal = pet_sprite_minimal(pose);
            assert_sprite_shape(&format!("Minimal {pose:?}"), &minimal[..], 9, 6);
            assert_eq!(
                occupied_component_count(compact),
                1,
                "Compact {pose:?} must be one complete mascot component"
            );
            assert_eq!(
                occupied_component_count(&minimal[..]),
                1,
                "Minimal {pose:?} must be one complete mascot component"
            );
        }
    }

    #[test]
    fn compact_idle_has_ears_face_gap_grounded_feet_and_attached_tail() {
        let sprite = pet_sprite_compact(PetPose::Idle);
        assert_sprite_shape("Compact Idle", sprite, 12, 10);
        assert!(
            has_two_pointed_ear_peaks(sprite),
            "compact idle needs two deep ear peaks"
        );
        assert!(
            has_two_wide_eye_gaps(sprite),
            "compact idle needs two wide negative-space eyes"
        );
        assert!(
            has_mouth_gap(sprite, 3),
            "compact idle needs a lower mouth gap"
        );
        let rows = occupied_row_bounds(sprite);
        let max_x = rows
            .iter()
            .flatten()
            .map(|(_, max_x)| *max_x)
            .max()
            .unwrap();
        let tail_rows = rows
            .iter()
            .rev()
            .take(2)
            .filter(|bounds| bounds.is_some_and(|(_, row_max)| row_max == max_x))
            .count();
        assert!(
            tail_rows == 2,
            "compact idle needs a grounded, attached tail contour"
        );
    }

    #[test]
    fn minimal_pose_families_have_distinct_complete_grids() {
        let neutral = pet_sprite_minimal(PetPose::Idle);
        for pose in [
            PetPose::WalkA,
            PetPose::WalkB,
            PetPose::Sit,
            PetPose::Curious,
        ] {
            assert_eq!(
                pet_sprite_minimal(pose),
                neutral,
                "{pose:?} should be neutral"
            );
        }

        let closed_eyes = pet_sprite_minimal(PetPose::Blink);
        for pose in [PetPose::Doze, PetPose::Sleep] {
            assert_eq!(
                pet_sprite_minimal(pose),
                closed_eyes,
                "{pose:?} should use the closed-eyes family"
            );
        }

        let positive = pet_sprite_minimal(PetPose::Happy);
        assert_eq!(pet_sprite_minimal(PetPose::Petted), positive);

        let families = [
            neutral,
            closed_eyes,
            positive,
            pet_sprite_minimal(PetPose::Upset),
            pet_sprite_minimal(PetPose::Yawn),
            pet_sprite_minimal(PetPose::Eating),
        ];
        for (index, first) in families.iter().enumerate() {
            for second in families.iter().skip(index + 1) {
                assert_ne!(first, second, "Minimal expression families must differ");
            }
        }
    }

    #[test]
    fn upright_poses_have_two_pointed_ear_peaks() {
        for pose in [
            PetPose::Idle,
            PetPose::Blink,
            PetPose::WalkA,
            PetPose::WalkB,
            PetPose::Sit,
            PetPose::Yawn,
            PetPose::Curious,
            PetPose::Happy,
            PetPose::Petted,
        ] {
            assert!(
                has_two_pointed_ear_peaks(pet_sprite(pose)),
                "{pose:?} needs two ear peaks with at least two rows of depth"
            );
        }
    }

    #[test]
    fn idle_face_has_wide_negative_space_features() {
        let idle = pet_sprite(PetPose::Idle);
        assert!(
            has_two_wide_eye_gaps(idle),
            "Idle needs two separated eye gaps at least two cells wide"
        );
        assert!(
            has_mouth_gap(idle, 3),
            "Idle needs a lower crooked mouth gap at least three cells wide"
        );
    }

    #[test]
    fn idle_and_blink_preserve_the_outer_silhouette() {
        let idle = pet_sprite(PetPose::Idle);
        let blink = pet_sprite(PetPose::Blink);
        assert_eq!(
            occupied_row_bounds(idle),
            occupied_row_bounds(blink),
            "Blink must preserve Idle ears, body sides, feet, and tail"
        );
        assert_ne!(
            face_space_runs(idle),
            face_space_runs(blink),
            "Blink must change eye gaps while preserving the outer contour"
        );
    }

    #[test]
    fn key_expression_signatures_are_named_and_distinct() {
        let happy = expression_signature(pet_sprite(PetPose::Happy));
        let petted = expression_signature(pet_sprite(PetPose::Petted));
        let upset = expression_signature(pet_sprite(PetPose::Upset));
        let eating = expression_signature(pet_sprite(PetPose::Eating));

        assert!(
            happy.high_raised_tail && happy.crescent_eyes && happy.broad_smile,
            "Happy needs crescent eyes, a broad smile, and a high raised tail: {happy:?}"
        );
        assert!(
            petted.broad_smile && petted.relaxed_shoulders,
            "Petted needs a broad smile and relaxed shoulder contour"
        );
        assert!(
            upset.lowered_ears && upset.downturned_mouth && upset.tucked_tail,
            "Upset needs lowered ears, a downturned mouth, and a tucked tail"
        );
        assert!(
            eating.food_facing_asymmetry,
            "Eating needs a food-facing asymmetric face and attached body"
        );

        for (first_name, first) in [
            ("Happy", happy),
            ("Petted", petted),
            ("Upset", upset),
            ("Eating", eating),
        ] {
            for (second_name, second) in [
                ("Happy", happy),
                ("Petted", petted),
                ("Upset", upset),
                ("Eating", eating),
            ] {
                if first_name != second_name {
                    assert_ne!(
                        first, second,
                        "{first_name} and {second_name} need distinct semantic contours"
                    );
                }
            }
        }
    }

    #[test]
    fn compact_yawn_and_curious_keep_named_face_contours() {
        let idle = pet_sprite_compact(PetPose::Idle);
        let yawn = pet_sprite_compact(PetPose::Yawn);
        let curious = pet_sprite_compact(PetPose::Curious);
        assert!(
            has_mouth_gap(yawn, 4),
            "Compact Yawn needs a visibly larger open-mouth gap"
        );
        assert!(
            occupied_row_bounds(curious)
                .iter()
                .find_map(|bounds| *bounds)
                .is_some_and(|(_, max_x)| {
                    let idle_top_max = occupied_row_bounds(idle)
                        .iter()
                        .find_map(|bounds| *bounds)
                        .map_or(0, |(_, max_x)| max_x);
                    max_x > idle_top_max
                }),
            "Compact Curious needs an asymmetric raised ear contour"
        );
        assert_ne!(
            occupied_row_bounds(yawn),
            occupied_row_bounds(idle),
            "Compact Yawn must change the mouth/body contour"
        );
        assert_ne!(
            occupied_row_bounds(curious),
            occupied_row_bounds(idle),
            "Compact Curious must change the ear/tail contour"
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
        let sprite = pet_sprite(PetPose::Doze);
        let (min_x, _, max_x, max_y) = occupied_bounds(sprite);
        assert!(max_x - min_x + 1 >= 16);
        let min_y = occupied_bounds(sprite).1;
        assert!(max_y - min_y < 10);
        let rows = occupied_row_bounds(sprite);
        assert!(
            rows.iter().any(|bounds| {
                bounds.is_some_and(|(row_min, row_max)| row_max - row_min + 1 >= 4)
            }),
            "doze needs a curled lower body"
        );
        assert!(
            rows.iter().skip(min_y).take(3).any(|bounds| {
                bounds.is_some_and(|(row_min, row_max)| row_max - row_min + 1 >= 3)
            }),
            "doze needs a wrapped upper tail cue"
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
        assert!(sleep.contains('o'), "bed sleep should retain belly detail");
        assert!(
            sleep.contains(' '),
            "bed sleep needs negative-space face detail"
        );
    }
}

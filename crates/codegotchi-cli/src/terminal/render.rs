use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};

use super::CodexScreen;

/// Paints the visible Codex virtual terminal into a Ratatui buffer area.
///
/// The source screen is borrowed immutably. Every destination cell in the
/// clipped area is reset before its VT style is applied, so rendering over an
/// existing buffer cannot retain stale symbols or attributes. The returned
/// cursor is in global Ratatui coordinates and is `None` when the child
/// cursor is hidden or outside the clipped area.
#[must_use]
pub fn render_codex(screen: &CodexScreen, area: Rect, buffer: &mut Buffer) -> Option<Position> {
    let vt_screen = screen.screen();
    let (rows, columns) = vt_screen.size();
    let visible_rows = rows.min(area.height);
    let visible_columns = columns.min(area.width);

    for row in 0..visible_rows {
        for column in 0..visible_columns {
            let Some(x) = area.x.checked_add(column) else {
                continue;
            };
            let Some(y) = area.y.checked_add(row) else {
                continue;
            };
            let Some(source) = vt_screen.cell(row, column) else {
                continue;
            };

            let continuation_in_area = column
                .checked_add(1)
                .is_some_and(|next_column| next_column < visible_columns);
            let continuation_in_buffer = continuation_in_area
                && x.checked_add(1)
                    .is_some_and(|next_x| buffer.cell(Position { x: next_x, y }).is_some());

            let Some(destination) = buffer.cell_mut(Position { x, y }) else {
                continue;
            };

            let mut symbol = if source.has_contents() {
                source.contents()
            } else {
                " "
            };

            // A wide lead must have its continuation inside the clipped area
            // before it is emitted. Otherwise the terminal would paint one
            // column beyond the compositor's ownership boundary.
            if source.is_wide() && !continuation_in_buffer {
                symbol = " ";
            }
            // vt100 continuation cells carry no glyph of their own. A space
            // preserves their style while avoiding duplicate wide glyphs.
            if source.is_wide_continuation() {
                symbol = " ";
            }

            destination.reset();
            destination.set_symbol(symbol).set_style(cell_style(source));
        }
    }

    if vt_screen.hide_cursor() {
        return None;
    }

    let (cursor_row, cursor_column) = vt_screen.cursor_position();
    if cursor_row >= visible_rows || cursor_column >= visible_columns {
        return None;
    }
    let x = area.x.checked_add(cursor_column)?;
    let y = area.y.checked_add(cursor_row)?;
    if buffer.cell(Position { x, y }).is_some() {
        Some(Position { x, y })
    } else {
        None
    }
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut modifiers = Modifier::empty();
    if cell.bold() {
        modifiers.insert(Modifier::BOLD);
    }
    if cell.dim() {
        modifiers.insert(Modifier::DIM);
    }
    if cell.italic() {
        modifiers.insert(Modifier::ITALIC);
    }
    if cell.underline() {
        modifiers.insert(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        modifiers.insert(Modifier::REVERSED);
    }

    Style::default()
        .fg(map_color(cell.fgcolor()))
        .bg(map_color(cell.bgcolor()))
        .add_modifier(modifiers)
}

fn map_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index @ 0..=15) => match index {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::White,
            _ => unreachable!("ANSI palette range is exhaustive"),
        },
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

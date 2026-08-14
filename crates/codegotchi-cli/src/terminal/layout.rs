use ratatui::layout::Rect;

const CODEX_MIN_HEIGHT: u16 = 18;
const FULL_ENTRY_HEIGHT: u16 = 40;
const FULL_RETAIN_HEIGHT: u16 = 36;
const COMPACT_ENTRY_HEIGHT: u16 = 26;
const COMPACT_RETAIN_HEIGHT: u16 = 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomLayoutMode {
    Full,
    Compact,
    Minimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalLayout {
    pub codex: Rect,
    pub room: Rect,
    pub room_mode: RoomLayoutMode,
}

pub fn choose_layout(terminal: Rect, previous: Option<RoomLayoutMode>) -> TerminalLayout {
    let room_mode = choose_mode(terminal.height, previous);
    let room_target = room_target_height(room_mode);
    let room_height = match room_mode {
        RoomLayoutMode::Minimal => terminal.height.min(room_target),
        RoomLayoutMode::Full | RoomLayoutMode::Compact => terminal
            .height
            .saturating_sub(CODEX_MIN_HEIGHT)
            .min(room_target),
    };
    let codex_height = terminal.height.saturating_sub(room_height);
    let room_y = terminal.y.saturating_add(codex_height);

    TerminalLayout {
        codex: Rect::new(terminal.x, terminal.y, terminal.width, codex_height),
        room: Rect::new(terminal.x, room_y, terminal.width, room_height),
        room_mode,
    }
}

fn choose_mode(height: u16, previous: Option<RoomLayoutMode>) -> RoomLayoutMode {
    match previous {
        None => ordinary_mode(height),
        Some(RoomLayoutMode::Full) => {
            if height >= FULL_RETAIN_HEIGHT {
                RoomLayoutMode::Full
            } else {
                ordinary_mode(height)
            }
        }
        Some(RoomLayoutMode::Compact) => {
            if height >= FULL_ENTRY_HEIGHT {
                RoomLayoutMode::Full
            } else if height >= COMPACT_RETAIN_HEIGHT {
                RoomLayoutMode::Compact
            } else {
                RoomLayoutMode::Minimal
            }
        }
        Some(RoomLayoutMode::Minimal) => ordinary_mode(height),
    }
}

fn ordinary_mode(height: u16) -> RoomLayoutMode {
    if height >= FULL_ENTRY_HEIGHT {
        RoomLayoutMode::Full
    } else if height >= COMPACT_ENTRY_HEIGHT {
        RoomLayoutMode::Compact
    } else {
        RoomLayoutMode::Minimal
    }
}

fn room_target_height(mode: RoomLayoutMode) -> u16 {
    match mode {
        RoomLayoutMode::Full => 14,
        RoomLayoutMode::Compact => 7,
        RoomLayoutMode::Minimal => 3,
    }
}

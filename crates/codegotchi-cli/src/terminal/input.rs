use codegotchi_domain::SimulationSnapshot;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use uuid::Uuid;

use super::behavior::PresentationFrame;
use super::room::room_geometry_with_frame;
use super::{CodexInputModes, MouseEncoding, MouseTrackingMode};

/// Backend-compatible pointer-distance scale for one terminal-cell path unit.
///
/// The conversion is a stable logical interaction unit, not an inference of
/// physical pixels: each Euclidean terminal-cell path unit contributes
/// `16.0` to the backend `pointer_distance` metric, whose thresholds stay
/// `1_500 ms` and `120.0`.
pub const POINTER_DISTANCE_PER_CELL: f32 = 16.0;
/// Minimum gesture duration for a pet to count, per the locked backend
/// contract. Strokes only start once a gesture qualifies.
pub const PET_MIN_INTERACTION_MS: u64 = 1_500;
/// Minimum pointer path length (in backend units) for a pet to count.
pub const PET_MIN_POINTER_DISTANCE: f32 = 120.0;

/// Sums Euclidean terminal-cell path lengths and converts them to the
/// backend pointer-distance metric.
#[must_use]
pub fn pointer_distance(path: &[Position]) -> f32 {
    path.windows(2)
        .map(|segment| {
            let dx = f32::from(segment[1].x) - f32::from(segment[0].x);
            let dy = f32::from(segment[1].y) - f32::from(segment[0].y);
            dx.hypot(dy)
        })
        .sum::<f32>()
        * POINTER_DISTANCE_PER_CELL
}

/// Accumulates one pointer-down-to-pointer-up petting gesture in terminal
/// cell coordinates. The local threshold gate controls when feedback is
/// emitted, while the authoritative domain independently validates the
/// cumulative evidence on every care request.
#[derive(Clone, Debug, Default)]
pub struct PetGesture {
    started_at: Option<std::time::Instant>,
    last: Option<Position>,
    cells: f32,
}

impl PetGesture {
    /// Begins a gesture at the pointer-down cell.
    pub fn begin(point: Position) -> Self {
        Self {
            started_at: Some(std::time::Instant::now()),
            last: Some(point),
            cells: 0.0,
        }
    }

    /// Extends the gesture path with a new pointer position.
    pub fn move_to(&mut self, point: Position) {
        if let Some(last) = self.last {
            let dx = f32::from(point.x) - f32::from(last.x);
            let dy = f32::from(point.y) - f32::from(last.y);
            self.cells += dx.hypot(dy);
        }
        self.last = Some(point);
    }

    /// Completes the gesture, returning `(interaction_ms, pointer_distance)`.
    #[must_use]
    pub fn finish(self) -> (u64, f32) {
        self.metrics()
    }

    /// Current `(interaction_ms, pointer_distance)` without consuming the
    /// gesture, so the session can keep emitting strokes while it is active.
    #[must_use]
    pub fn metrics(&self) -> (u64, f32) {
        let interaction_ms = self
            .started_at
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);
        (interaction_ms, self.cells * POINTER_DISTANCE_PER_CELL)
    }

    /// Whether this gesture already meets the backend pet thresholds.
    #[must_use]
    pub fn qualified(&self) -> bool {
        let (interaction_ms, pointer_distance) = self.metrics();
        interaction_ms >= PET_MIN_INTERACTION_MS && pointer_distance >= PET_MIN_POINTER_DISTANCE
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }
}

/// One validated care request produced by the room's mouse surface. The
/// terminal never mutates simulation state directly; every request goes
/// through the authoritative runtime gateway.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomCareRequest {
    Feed {
        action_id: Uuid,
        food_id: String,
    },
    Clean {
        action_id: Uuid,
        poop_id: Uuid,
    },
    Nap {
        action_id: Uuid,
    },
    Pet {
        action_id: Uuid,
        interaction_ms: u64,
        pointer_distance: f32,
    },
    /// One increment of an active, locally qualified petting gesture. The
    /// cumulative evidence is carried to the authoritative runtime, which
    /// independently validates it before mutating happiness.
    PetStroke {
        action_id: Uuid,
        duration_ms: u64,
        distance: f64,
    },
}

/// Injection seam for authoritative care requests. The production
/// implementation is the `AuthoritativeRuntime`; tests can record requests
/// without a database.
pub trait CareGateway {
    fn feed(&self, action_id: Uuid, food_id: &str);
    fn clean(&self, action_id: Uuid, poop_id: Uuid);
    fn nap(&self, action_id: Uuid);
    fn pet(&self, action_id: Uuid, interaction_ms: u64, pointer_distance: f32);
    fn pet_stroke(&self, action_id: Uuid, duration_ms: u64, distance: f64);
}

/// Pure room-mouse state machine. It owns petting-gesture and food-drag state
/// and converts pane-routed mouse events into authoritative care requests.
/// Room events never produce Codex input bytes; the caller routes by pane.
#[derive(Clone, Debug, Default)]
pub struct RoomInputSession {
    gesture: Option<PetGesture>,
    dragging_food: Option<&'static str>,
    drag_position: Option<Position>,
}

impl RoomInputSession {
    /// Processes one mouse event inside the room rectangle and returns any
    /// completed care requests. `snapshot` is the latest authoritative state.
    #[must_use]
    pub fn process(
        &mut self,
        room: Rect,
        snapshot: &SimulationSnapshot,
        frame: &PresentationFrame,
        event: &MouseEvent,
    ) -> Vec<RoomCareRequest> {
        let geometry = room_geometry_with_frame(room, snapshot, frame);
        let point = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(food_id) = geometry.food_hit(point) {
                    self.dragging_food = Some(food_id);
                    self.drag_position = Some(point);
                    self.gesture = None;
                    return Vec::new();
                }
                if geometry.pet.contains(point) {
                    self.gesture = Some(PetGesture::begin(point));
                    self.dragging_food = None;
                    self.drag_position = None;
                }
                Vec::new()
            }
            MouseEventKind::Drag(_) => {
                if let Some(gesture) = self.gesture.as_mut() {
                    gesture.move_to(point);
                }
                if self.dragging_food.is_some() {
                    self.drag_position = Some(point);
                }
                Vec::new()
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let mut requests = Vec::new();
                self.drag_position = None;
                if let Some(food_id) = self.dragging_food.take() {
                    // Every layout uses the same drag-to-pet interaction:
                    // dropping anywhere else cancels the drag.
                    if geometry.pet.contains(point) {
                        requests.push(RoomCareRequest::Feed {
                            action_id: Uuid::new_v4(),
                            food_id: food_id.to_owned(),
                        });
                    }
                    return requests;
                }
                if let Some(mut gesture) = self.gesture.take() {
                    // Include the pointer-up position as the final path point
                    // so quick drags without an intermediate Drag event still
                    // accumulate distance.
                    gesture.move_to(point);
                    let (interaction_ms, pointer_distance) = gesture.finish();
                    if interaction_ms >= PET_MIN_INTERACTION_MS
                        && pointer_distance >= PET_MIN_POINTER_DISTANCE
                    {
                        requests.push(RoomCareRequest::Pet {
                            action_id: Uuid::new_v4(),
                            interaction_ms,
                            pointer_distance,
                        });
                    }
                    return requests;
                }
                if let Some(poop_id) = geometry.poop_hit(point) {
                    requests.push(RoomCareRequest::Clean {
                        action_id: Uuid::new_v4(),
                        poop_id,
                    });
                    return requests;
                }
                if geometry.bed.is_some_and(|bed| bed.contains(point)) {
                    requests.push(RoomCareRequest::Nap {
                        action_id: Uuid::new_v4(),
                    });
                }
                requests
            }
            MouseEventKind::Up(_)
            | MouseEventKind::Down(_)
            | MouseEventKind::Moved
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollUp => Vec::new(),
        }
    }

    /// The active food drag (food id + current pointer cell), used by the
    /// renderer to draw the drag ghost. Returns `None` when no drag is active.
    #[must_use]
    pub fn active_drag(&self) -> Option<(&'static str, Position)> {
        self.dragging_food.zip(self.drag_position)
    }

    /// True while a petting gesture is active and already meets the backend
    /// duration/distance contract. The session uses this to keep applying
    /// authoritative happiness strokes each tick until the gesture ends.
    #[must_use]
    pub fn petting_qualified(&self) -> bool {
        self.petting_evidence().is_some()
    }

    /// Returns cumulative evidence for an active gesture once the local
    /// presentation gate is met. This is measurement only: the runtime/domain
    /// validates the same evidence again before every happiness mutation.
    #[must_use]
    pub fn petting_evidence(&self) -> Option<(u64, f64)> {
        let gesture = self.gesture.as_ref()?;
        let (duration_ms, distance) = gesture.metrics();
        (duration_ms >= PET_MIN_INTERACTION_MS && distance >= PET_MIN_POINTER_DISTANCE)
            .then_some((duration_ms, f64::from(distance)))
    }
}

/// Encodes one crossterm key event for the currently negotiated Codex modes.
#[must_use]
pub fn encode_key_event(event: KeyEvent, modes: CodexInputModes) -> Vec<u8> {
    if matches!(event.kind, KeyEventKind::Release) {
        return Vec::new();
    }

    let modifiers = event.modifiers;
    let alt = modifiers.contains(KeyModifiers::ALT);
    let alt_prefix = alt
        && matches!(
            event.code,
            KeyCode::Char(_)
                | KeyCode::Enter
                | KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Backspace
                | KeyCode::Esc
                | KeyCode::Null
        );
    let mut bytes = match event.code {
        KeyCode::Char(character) => encode_character(character, modifiers),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => sequence(b"\x1b[Z"),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Null => vec![0],
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Insert
        | KeyCode::Delete => encode_navigation(event.code, modifiers, modes),
        KeyCode::F(number) => encode_function(number, modifiers),
        _ => Vec::new(),
    };

    if alt_prefix {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.append(&mut bytes);
        prefixed
    } else {
        bytes
    }
}

/// Encodes a paste payload using the negotiated bracketed-paste mode.
#[must_use]
pub fn encode_paste(content: &str, modes: CodexInputModes) -> Vec<u8> {
    if !modes.bracketed_paste {
        return content.as_bytes().to_vec();
    }

    let mut bytes = Vec::with_capacity(content.len() + 12);
    bytes.extend_from_slice(b"\x1b[200~");
    bytes.extend_from_slice(content.as_bytes());
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}

/// Encodes focus gained/lost using the negotiated DEC focus-reporting mode.
#[must_use]
pub fn encode_focus_event(gained: bool, modes: CodexInputModes) -> Vec<u8> {
    if !modes.focus_reporting {
        return Vec::new();
    }
    if gained {
        sequence(b"\x1b[I")
    } else {
        sequence(b"\x1b[O")
    }
}

/// Encodes a pane-relative crossterm mouse event for negotiated Codex modes.
/// Coordinates are zero-based at the API boundary and one-based on the wire.
#[must_use]
pub fn encode_mouse_event(event: MouseEvent, modes: CodexInputModes) -> Vec<u8> {
    if !tracking_allows(modes.mouse_tracking, event.kind) {
        return Vec::new();
    }

    let Some(x) = event.column.checked_add(1) else {
        return Vec::new();
    };
    let Some(y) = event.row.checked_add(1) else {
        return Vec::new();
    };
    let Some(code) = mouse_code(
        event.kind,
        event.modifiers,
        matches!(modes.mouse_encoding, MouseEncoding::Sgr),
    ) else {
        return Vec::new();
    };

    match modes.mouse_encoding {
        MouseEncoding::Default => encode_default_mouse(code, x, y),
        MouseEncoding::Utf8 => encode_utf8_mouse(code, x, y),
        MouseEncoding::Sgr => encode_sgr_mouse(code, x, y, event.kind),
    }
}

/// Short alias kept for callers that treat input encoders as protocol
/// primitives rather than crossterm event adapters.
#[must_use]
pub fn encode_key(event: KeyEvent, modes: CodexInputModes) -> Vec<u8> {
    encode_key_event(event, modes)
}

/// Short alias for [`encode_focus_event`].
#[must_use]
pub fn encode_focus(gained: bool, modes: CodexInputModes) -> Vec<u8> {
    encode_focus_event(gained, modes)
}

/// Short alias for [`encode_mouse_event`].
#[must_use]
pub fn encode_mouse(event: MouseEvent, modes: CodexInputModes) -> Vec<u8> {
    encode_mouse_event(event, modes)
}

fn sequence(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

fn encode_character(character: char, modifiers: KeyModifiers) -> Vec<u8> {
    if modifiers.contains(KeyModifiers::CONTROL)
        && let Some(control) = control_byte(character)
    {
        return vec![control];
    }

    let character = if modifiers.contains(KeyModifiers::SHIFT) && character.is_ascii_lowercase() {
        character.to_ascii_uppercase()
    } else {
        character
    };
    let mut bytes = [0; 4];
    character.encode_utf8(&mut bytes).as_bytes().to_vec()
}

fn control_byte(character: char) -> Option<u8> {
    let byte = character as u32;
    match byte {
        0x40..=0x5f => Some((byte - 0x40) as u8),
        0x60..=0x7f => Some((byte - 0x60) as u8),
        0x20 | 0x3f => Some(if byte == 0x20 { 0 } else { 0x7f }),
        _ => None,
    }
}

fn encode_navigation(code: KeyCode, modifiers: KeyModifiers, modes: CodexInputModes) -> Vec<u8> {
    let modifier = modifier_parameter(modifiers);
    match code {
        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
            let final_byte = match code {
                KeyCode::Up => b'A',
                KeyCode::Down => b'B',
                KeyCode::Right => b'C',
                KeyCode::Left => b'D',
                _ => unreachable!(),
            };
            if modifier == 1 && modes.application_cursor_keys {
                vec![0x1b, b'O', final_byte]
            } else if modifier == 1 {
                vec![0x1b, b'[', final_byte]
            } else {
                csi_with_modifier(modifier, final_byte)
            }
        }
        KeyCode::Home | KeyCode::End => {
            let final_byte = if matches!(code, KeyCode::Home) {
                b'H'
            } else {
                b'F'
            };
            if modifier == 1 && modes.application_cursor_keys {
                vec![0x1b, b'O', final_byte]
            } else if modifier == 1 {
                vec![0x1b, b'[', final_byte]
            } else {
                csi_with_modifier(modifier, final_byte)
            }
        }
        KeyCode::PageUp => csi_tilde(modifier, 5),
        KeyCode::PageDown => csi_tilde(modifier, 6),
        KeyCode::Insert => csi_tilde(modifier, 2),
        KeyCode::Delete => csi_tilde(modifier, 3),
        _ => Vec::new(),
    }
}

fn encode_function(number: u8, modifiers: KeyModifiers) -> Vec<u8> {
    if number == 0 {
        return Vec::new();
    }
    let modifier = modifier_parameter(modifiers);
    let (final_byte, parameter) = match number {
        1 => (b'P', None),
        2 => (b'Q', None),
        3 => (b'R', None),
        4 => (b'S', None),
        5 => (b'~', Some(15)),
        6 => (b'~', Some(17)),
        7 => (b'~', Some(18)),
        8 => (b'~', Some(19)),
        9 => (b'~', Some(20)),
        10 => (b'~', Some(21)),
        11 => (b'~', Some(23)),
        12 => (b'~', Some(24)),
        13 => (b'~', Some(25)),
        14 => (b'~', Some(26)),
        15 => (b'~', Some(28)),
        16 => (b'~', Some(29)),
        17 => (b'~', Some(31)),
        18 => (b'~', Some(32)),
        19 => (b'~', Some(33)),
        20 => (b'~', Some(34)),
        21 => (b'~', Some(35)),
        22 => (b'~', Some(36)),
        23 => (b'~', Some(37)),
        24 => (b'~', Some(38)),
        _ => return Vec::new(),
    };
    if modifier == 1 {
        if let Some(parameter) = parameter {
            return csi_tilde(1, parameter);
        }
        return vec![0x1b, b'O', final_byte];
    }
    if let Some(parameter) = parameter {
        csi_tilde(modifier, parameter)
    } else {
        csi_with_modifier(modifier, final_byte)
    }
}

fn modifier_parameter(modifiers: KeyModifiers) -> u8 {
    1 + u8::from(modifiers.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
        + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL))
}

fn csi_with_modifier(modifier: u8, final_byte: u8) -> Vec<u8> {
    let mut bytes = vec![0x1b, b'['];
    bytes.extend_from_slice(b"1;");
    bytes.extend_from_slice(modifier.to_string().as_bytes());
    bytes.push(final_byte);
    bytes
}

fn csi_tilde(modifier: u8, parameter: u16) -> Vec<u8> {
    let mut bytes = vec![0x1b, b'['];
    bytes.extend_from_slice(parameter.to_string().as_bytes());
    if modifier != 1 {
        bytes.push(b';');
        bytes.extend_from_slice(modifier.to_string().as_bytes());
    }
    bytes.push(b'~');
    bytes
}

fn tracking_allows(tracking: MouseTrackingMode, kind: MouseEventKind) -> bool {
    match tracking {
        MouseTrackingMode::Disabled => false,
        MouseTrackingMode::Press => {
            matches!(
                kind,
                MouseEventKind::Down(_)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            )
        }
        MouseTrackingMode::PressRelease => {
            matches!(
                kind,
                MouseEventKind::Down(_)
                    | MouseEventKind::Up(_)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            )
        }
        MouseTrackingMode::ButtonMotion => {
            matches!(
                kind,
                MouseEventKind::Down(_)
                    | MouseEventKind::Up(_)
                    | MouseEventKind::Drag(_)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            )
        }
        MouseTrackingMode::AnyMotion => true,
    }
}

fn mouse_code(kind: MouseEventKind, modifiers: KeyModifiers, sgr_release: bool) -> Option<u16> {
    let button = match kind {
        MouseEventKind::Down(button) => button_code(button),
        MouseEventKind::Drag(button) => 32 + button_code(button),
        MouseEventKind::Up(button) => {
            if sgr_release {
                button_code(button)
            } else {
                3
            }
        }
        MouseEventKind::Moved => 35,
        MouseEventKind::ScrollUp => 64,
        MouseEventKind::ScrollDown => 65,
        MouseEventKind::ScrollLeft => 66,
        MouseEventKind::ScrollRight => 67,
    };
    let modifier = u16::from(modifiers.contains(KeyModifiers::SHIFT)) * 4
        + u16::from(modifiers.contains(KeyModifiers::ALT)) * 8
        + u16::from(modifiers.contains(KeyModifiers::CONTROL)) * 16;
    Some(button + modifier)
}

fn button_code(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

fn encode_default_mouse(code: u16, x: u16, y: u16) -> Vec<u8> {
    if x > 223 || y > 223 || code > 223 {
        return Vec::new();
    }
    vec![
        0x1b,
        b'[',
        b'M',
        (code + 32) as u8,
        (x + 32) as u8,
        (y + 32) as u8,
    ]
}

fn encode_utf8_mouse(code: u16, x: u16, y: u16) -> Vec<u8> {
    if x > 2_015 || y > 2_015 || code > 2_015 {
        return Vec::new();
    }
    let mut bytes = vec![0x1b, b'[', b'M'];
    append_utf8_codepoint(&mut bytes, code + 32);
    append_utf8_codepoint(&mut bytes, x + 32);
    append_utf8_codepoint(&mut bytes, y + 32);
    bytes
}

fn append_utf8_codepoint(bytes: &mut Vec<u8>, value: u16) {
    let character = char::from_u32(u32::from(value)).expect("mouse codepoint is valid");
    let mut encoded = [0; 4];
    bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
}

fn encode_sgr_mouse(code: u16, x: u16, y: u16, kind: MouseEventKind) -> Vec<u8> {
    let mut bytes = vec![0x1b, b'[', b'<'];
    bytes.extend_from_slice(code.to_string().as_bytes());
    bytes.push(b';');
    bytes.extend_from_slice(x.to_string().as_bytes());
    bytes.push(b';');
    bytes.extend_from_slice(y.to_string().as_bytes());
    bytes.push(if matches!(kind, MouseEventKind::Up(_)) {
        b'm'
    } else {
        b'M'
    });
    bytes
}

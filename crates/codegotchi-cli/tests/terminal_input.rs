use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use codegotchi_cli::terminal::{
    CareGateway, POINTER_DISTANCE_PER_CELL, PetGesture, RoomCareRequest, RoomInputSession,
    pointer_distance,
};
use codegotchi_domain::{
    DefaultNeedProgressionStrategy, FoodInventory, Pet, PetSimulation, PetSpecies, Poop,
    SimulationSnapshot, SystemClock,
};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use uuid::Uuid;

fn base_snapshot() -> SimulationSnapshot {
    let now = Utc::now();
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot()
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// The terminal-to-backend petting conversion is a locked stable unit: every
/// Euclidean terminal-cell path unit contributes exactly `16.0`.
#[test]
fn pointer_distance_uses_exact_cell_scale() {
    assert_eq!(POINTER_DISTANCE_PER_CELL, 16.0);

    let horizontal_7 = (0..=7).map(|x| Position::new(x, 0)).collect::<Vec<_>>();
    assert_eq!(pointer_distance(&horizontal_7), 112.0);

    let horizontal_8 = (0..=8).map(|x| Position::new(x, 0)).collect::<Vec<_>>();
    assert_eq!(pointer_distance(&horizontal_8), 128.0);

    let diagonal_3_4 = [Position::new(0, 0), Position::new(3, 4)];
    assert_eq!(pointer_distance(&diagonal_3_4), 80.0);
}

/// A pet gesture accumulates Euclidean cell path length through the same
/// scale as the pure conversion.
#[test]
fn pet_gesture_accumulates_terminal_cell_distance() {
    let mut gesture = PetGesture::begin(Position::new(0, 0));
    for x in 1..=8 {
        gesture.move_to(Position::new(x, 0));
    }
    let (_, distance) = gesture.finish();
    assert_eq!(distance, 128.0);
}

#[derive(Default)]
struct RecordingCareGateway {
    requests: Mutex<Vec<RoomCareRequest>>,
}

impl CareGateway for RecordingCareGateway {
    fn feed(&self, action_id: Uuid, food_id: &str) {
        self.requests.lock().unwrap().push(RoomCareRequest::Feed {
            action_id,
            food_id: food_id.to_owned(),
        });
    }

    fn clean(&self, action_id: Uuid, poop_id: Uuid) {
        self.requests
            .lock()
            .unwrap()
            .push(RoomCareRequest::Clean { action_id, poop_id });
    }

    fn nap(&self, action_id: Uuid) {
        self.requests
            .lock()
            .unwrap()
            .push(RoomCareRequest::Nap { action_id });
    }

    fn pet(&self, action_id: Uuid, interaction_ms: u64, pointer_distance: f32) {
        self.requests.lock().unwrap().push(RoomCareRequest::Pet {
            action_id,
            interaction_ms,
            pointer_distance,
        });
    }
}

fn apply(gateway: &RecordingCareGateway, requests: Vec<RoomCareRequest>) {
    for request in requests {
        match request {
            RoomCareRequest::Feed { action_id, food_id } => {
                gateway.feed(action_id, &food_id);
            }
            RoomCareRequest::Clean { action_id, poop_id } => gateway.clean(action_id, poop_id),
            RoomCareRequest::Nap { action_id } => gateway.nap(action_id),
            RoomCareRequest::Pet {
                action_id,
                interaction_ms,
                pointer_distance,
            } => gateway.pet(action_id, interaction_ms, pointer_distance),
        }
    }
}

/// A qualifying pet gesture submits exactly one pet care request with the
/// backend thresholds (>= 1500 ms, >= 120.0 pointer distance).
#[test]
fn petting_gesture_submits_only_after_backend_thresholds() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 40, 14);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    // Begin on the pet (Full pet rect starts at x=6, y=4).
    let mut requests = input.process(
        room,
        &snapshot,
        &mouse(MouseEventKind::Down(MouseButton::Left), 10, 6),
    );
    apply(&gateway, requests);
    assert!(gateway.requests.lock().unwrap().is_empty());

    std::thread::sleep(Duration::from_millis(1_600));

    for point in [
        Position::new(12, 6),
        Position::new(14, 6),
        Position::new(16, 7),
        Position::new(18, 8),
    ] {
        requests = input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Drag(MouseButton::Left), point.x, point.y),
        );
        apply(&gateway, requests);
    }

    requests = input.process(
        room,
        &snapshot,
        &mouse(MouseEventKind::Up(MouseButton::Left), 18, 8),
    );
    apply(&gateway, requests);

    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one pet request expected");
    match &recorded[0] {
        RoomCareRequest::Pet {
            interaction_ms,
            pointer_distance,
            ..
        } => {
            assert!(*interaction_ms >= 1_500);
            assert!(*pointer_distance >= 120.0);
        }
        other => panic!("expected Pet request, got {other:?}"),
    }
}

/// A short gesture below both thresholds never reaches the runtime.
#[test]
fn short_pet_gesture_is_dropped() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 40, 14);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Down(MouseButton::Left), 10, 6),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 18, 8),
        ),
    );
    assert!(gateway.requests.lock().unwrap().is_empty());
}

/// Food drag-to-pet submits feed; dropping elsewhere cancels.
#[test]
fn food_drag_to_pet_feeds_and_other_drops_do_not() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 40, 14);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    // Down on the food tray (x=2..14, y=11..12), up on the pet (x=6..20, y=4..8).
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Down(MouseButton::Left), 4, 12),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 10, 6),
        ),
    );
    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert!(matches!(&recorded[0], RoomCareRequest::Feed { food_id, .. } if food_id == "kibble"));
    drop(recorded);

    let mut input = RoomInputSession::default();
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Down(MouseButton::Left), 4, 12),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 30, 12),
        ),
    );
    assert_eq!(
        gateway.requests.lock().unwrap().len(),
        1,
        "dropping food away from the pet must not feed"
    );
}

/// Clicking an authoritative poop submits clean with its id; the bed submits
/// nap; non-left buttons produce nothing.
#[test]
fn poop_click_cleans_and_bed_click_naps() {
    let mut snapshot = base_snapshot();
    let poop_id = Uuid::from_u128(0x700f);
    snapshot.pending_poops.push(Poop::new(poop_id, Utc::now()));
    let room = Rect::new(0, 0, 40, 14);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 16, 12),
        ),
    );
    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0],
        RoomCareRequest::Clean {
            action_id: action_id(&recorded[0]),
            poop_id,
        }
    );
    drop(recorded);

    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 30, 10),
        ),
    );
    let recorded = gateway.requests.lock().unwrap();
    assert!(matches!(&recorded[1], RoomCareRequest::Nap { .. }));
    drop(recorded);

    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Right), 30, 10),
        ),
    );
    assert_eq!(
        gateway.requests.lock().unwrap().len(),
        2,
        "non-left buttons must not trigger room care"
    );
}

/// Minimal keeps recovery possible: food down + any room up feeds.
#[test]
fn minimal_food_tray_feeds_from_any_room_release() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 40, 3);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 1),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &mouse(MouseEventKind::Up(MouseButton::Left), 20, 2),
        ),
    );
    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert!(matches!(&recorded[0], RoomCareRequest::Feed { .. }));
}

fn action_id(request: &RoomCareRequest) -> Uuid {
    match request {
        RoomCareRequest::Feed { action_id, .. }
        | RoomCareRequest::Clean { action_id, .. }
        | RoomCareRequest::Nap { action_id }
        | RoomCareRequest::Pet { action_id, .. } => *action_id,
    }
}

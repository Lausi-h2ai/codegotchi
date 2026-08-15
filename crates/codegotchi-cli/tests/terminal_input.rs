use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use codegotchi_cli::terminal::{
    CareGateway, POINTER_DISTANCE_PER_CELL, PetGesture, PetPose, PresentationFrame,
    RoomCareRequest, RoomInputSession, pointer_distance, room_geometry_with_frame,
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

fn default_frame() -> PresentationFrame {
    PresentationFrame::default()
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
        &default_frame(),
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
            &default_frame(),
            &mouse(MouseEventKind::Drag(MouseButton::Left), point.x, point.y),
        );
        apply(&gateway, requests);
    }

    requests = input.process(
        room,
        &snapshot,
        &default_frame(),
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

/// The pet hitbox follows the presentation wander offset: a gesture that
/// starts on the moved pet must still reach the authoritative runtime.
#[test]
fn petting_hitbox_follows_the_wandering_pet() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 120, 45);
    let frame = PresentationFrame {
        pose: PetPose::WalkA,
        offset: (-12, 3),
    };
    let geometry = room_geometry_with_frame(room, &snapshot, &frame);
    let pet = geometry.pet;
    assert!(
        pet.x < 120 && pet.y < 45,
        "moved pet hitbox must stay inside the room"
    );

    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();
    let start = Position::new(pet.x + 2, pet.y + 2);
    let mut requests = input.process(
        room,
        &snapshot,
        &frame,
        &mouse(MouseEventKind::Down(MouseButton::Left), start.x, start.y),
    );
    apply(&gateway, requests);
    assert!(gateway.requests.lock().unwrap().is_empty());

    std::thread::sleep(Duration::from_millis(1_600));
    let end = Position::new(start.x + 8, start.y);
    requests = input.process(
        room,
        &snapshot,
        &frame,
        &mouse(MouseEventKind::Up(MouseButton::Left), end.x, end.y),
    );
    apply(&gateway, requests);

    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let RoomCareRequest::Pet {
        interaction_ms,
        pointer_distance,
        ..
    } = &recorded[0]
    else {
        panic!("expected a pet care request, got {:?}", recorded[0]);
    };
    assert!(*interaction_ms >= 1_500);
    assert!(*pointer_distance >= 120.0);
}

/// Full exposes every stocked food kind as its own draggable source with its
/// authoritative count, and each drag submits the correct food id.
#[test]
fn every_stocked_food_is_a_draggable_source_with_count() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 120, 14);
    let geometry = room_geometry_with_frame(room, &snapshot, &default_frame());
    let mut sources: Vec<_> = geometry
        .food_sources
        .iter()
        .map(|source| (source.food_id, source.count))
        .collect();
    sources.sort();
    assert_eq!(
        sources,
        vec![
            ("energy_drink", 10),
            ("fruit", 25),
            ("kibble", 50),
            ("treat", 25),
        ]
    );

    for (food_id, _) in sources {
        let mut input = RoomInputSession::default();
        let gateway = RecordingCareGateway::default();
        let source = geometry
            .food_sources
            .iter()
            .find(|source| source.food_id == food_id)
            .expect("source exists");
        apply(
            &gateway,
            input.process(
                room,
                &snapshot,
                &default_frame(),
                &mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    source.rect.x + 1,
                    source.rect.y,
                ),
            ),
        );
        let pet = geometry.pet;
        apply(
            &gateway,
            input.process(
                room,
                &snapshot,
                &default_frame(),
                &mouse(MouseEventKind::Up(MouseButton::Left), pet.x + 2, pet.y + 2),
            ),
        );
        let recorded = gateway.requests.lock().unwrap();
        assert_eq!(recorded.len(), 1, "{food_id} should feed once");
        assert!(
            matches!(&recorded[0], RoomCareRequest::Feed { food_id: id, .. } if id == food_id),
            "{food_id} drag submitted {:?}",
            recorded[0]
        );
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
            &default_frame(),
            &mouse(MouseEventKind::Down(MouseButton::Left), 10, 6),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
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

    // Down on the kibble source (x=2..16, y=11), up on the pet.
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
            &mouse(MouseEventKind::Down(MouseButton::Left), 4, 11),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
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
            &default_frame(),
            &mouse(MouseEventKind::Down(MouseButton::Left), 4, 11),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
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
            &default_frame(),
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
            &default_frame(),
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
            &default_frame(),
            &mouse(MouseEventKind::Up(MouseButton::Right), 30, 10),
        ),
    );
    assert_eq!(
        gateway.requests.lock().unwrap().len(),
        2,
        "non-left buttons must not trigger room care"
    );
}

/// Minimal keeps recovery possible with the same strict drag-to-pet rule:
/// dropping on the pet feeds, dropping anywhere else cancels.
#[test]
fn minimal_food_tray_requires_drop_on_pet() {
    let snapshot = base_snapshot();
    let room = Rect::new(0, 0, 40, 3);
    let mut input = RoomInputSession::default();
    let gateway = RecordingCareGateway::default();

    // Drop on the pet (3x1 rect at the far left) feeds.
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 1),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
            &mouse(MouseEventKind::Up(MouseButton::Left), 1, 0),
        ),
    );
    let recorded = gateway.requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert!(matches!(&recorded[0], RoomCareRequest::Feed { .. }));
    drop(recorded);

    // Dropping anywhere else in the room must NOT feed.
    let mut input = RoomInputSession::default();
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
            &mouse(MouseEventKind::Down(MouseButton::Left), 3, 1),
        ),
    );
    apply(
        &gateway,
        input.process(
            room,
            &snapshot,
            &default_frame(),
            &mouse(MouseEventKind::Up(MouseButton::Left), 20, 2),
        ),
    );
    assert_eq!(
        gateway.requests.lock().unwrap().len(),
        1,
        "Minimal must use the same drag-to-pet rule: a drop outside the pet must not feed"
    );
}

fn action_id(request: &RoomCareRequest) -> Uuid {
    match request {
        RoomCareRequest::Feed { action_id, .. }
        | RoomCareRequest::Clean { action_id, .. }
        | RoomCareRequest::Nap { action_id }
        | RoomCareRequest::Pet { action_id, .. } => *action_id,
    }
}

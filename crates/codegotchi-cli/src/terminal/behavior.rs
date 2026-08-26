use codegotchi_domain::{ActivityKind, AgentActivityState, PetBehavior, SimulationSnapshot};
use ratatui::layout::{Position, Rect};
use std::time::Duration;

use super::room::{absolute_room_rect, offset_rect, wide_full_care_zone, wide_full_pet_home};

/// Broad, durable presentation moods derived exclusively from authoritative
/// structured state. The terminal renderer never classifies visible Codex
/// text; this mapping is the only activity projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentationActivity {
    #[default]
    Calm,
    Thinking,
    Working,
    Success,
    Failure,
    WaitingOrBlocked,
}

/// Maps one authoritative snapshot to its broad presentation activity.
///
/// The mapping is exact and exhaustive per the hardening spec: current
/// aggregate activity wins over stale recent outcomes, blocked/waiting states
/// always win, and `Idle` alone may fall back to a recent outcome.
#[must_use]
pub fn presentation_activity(snapshot: &SimulationSnapshot) -> PresentationActivity {
    match snapshot.activity {
        AgentActivityState::Blocked | AgentActivityState::WaitingForUser => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Idle) => PresentationActivity::Calm,
        AgentActivityState::Active(ActivityKind::Thinking) => PresentationActivity::Thinking,
        AgentActivityState::Active(ActivityKind::Waiting | ActivityKind::Blocked) => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Celebrating) => PresentationActivity::Success,
        AgentActivityState::Active(ActivityKind::Error) => PresentationActivity::Failure,
        AgentActivityState::Active(
            ActivityKind::Reading
            | ActivityKind::Searching
            | ActivityKind::Editing
            | ActivityKind::Testing
            | ActivityKind::Building
            | ActivityKind::Installing
            | ActivityKind::GitOperation
            | ActivityKind::DockerOperation
            | ActivityKind::WebResearch
            | ActivityKind::UnknownWork,
        ) => PresentationActivity::Working,
        AgentActivityState::Idle => match snapshot.behavior {
            PetBehavior::RecentSuccess => PresentationActivity::Success,
            PetBehavior::RecentFailure => PresentationActivity::Failure,
            _ => PresentationActivity::Calm,
        },
    }
}

/// Whether the snapshot currently represents an authoritative recovery nap.
///
/// `PetBehavior::Sleeping` alone is NOT authoritative sleep. Only an active
/// future `napping_until` is. A `Sleeping` snapshot without one is ordinary
/// idle behavior and must never use the recovery bed.
#[must_use]
pub fn has_authoritative_nap(snapshot: &SimulationSnapshot) -> bool {
    snapshot
        .napping_until
        .is_some_and(|until| snapshot.last_updated_at < until)
}

/// A pet pose projected by the terminal renderer. Poses are presentation-only
/// and never mutate authoritative state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PetPose {
    #[default]
    Idle,
    Blink,
    WalkA,
    WalkB,
    Sit,
    Doze,
    Yawn,
    Curious,
    Happy,
    Upset,
    Eating,
    Petted,
    Sleep,
}

/// Logical room anchors that autonomous inspection may approach. Furniture
/// silhouettes are not all rendered yet; anchors are deliberately independent
/// from the current visual furniture set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomObject {
    Window,
    Laptop,
    Shelf,
    Plants,
    Bed,
    Food,
}

/// One autonomous presentation intent. Deliberately contains NO care action:
/// the enum has no `Feed`, `Clean`, `Nap`, or `Pet` variants, so autonomous
/// behavior is structurally unable to repair authoritative state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleIntent {
    Wander(Position),
    Sit,
    Inspect(RoomObject),
    LookOutWindow,
    Yawn,
    WatchCodex,
    Celebrate,
    Worry,
}

/// The deterministic presentation projection for one render tick: the pet
/// pose and the room-relative cell offset from the layout's home position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationFrame {
    pub pose: PetPose,
    /// Cell offset from the layout home pet position; the pet hitbox follows
    /// this offset so mouse care stays aligned with the visible pet.
    pub offset: (i16, i16),
}

/// Deterministic, session-local presentation state.
///
/// All transitions come from a seeded PRNG, so the same seed and inputs
/// produce the same frames. Autonomous intents may express needs (linger near
/// food when hungry, doze/yawn near the bed when tired, seek attention when
/// lonely) but never submit care actions.
#[derive(Clone, Debug)]
pub struct PresentationState {
    rng: SplitMix64,
    frame: PresentationFrame,
    intent: IdleIntent,
    intent_started: Duration,
    intent_duration: Duration,
    target: Position,
    home: Position,
    phase: u8,
    next_blink: Duration,
    blink_until: Duration,
    reaction: Option<(PetPose, Duration)>,
}

impl PresentationState {
    /// Creates a presentation state with a fixed seed for deterministic tests.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            frame: PresentationFrame::default(),
            intent: IdleIntent::Sit,
            intent_started: Duration::ZERO,
            intent_duration: Duration::from_secs(1),
            target: Position::new(0, 0),
            home: Position::new(0, 0),
            phase: 0,
            next_blink: Duration::from_secs(2),
            blink_until: Duration::ZERO,
            reaction: None,
        }
    }

    /// The current deterministic frame.
    #[must_use]
    pub fn frame(&self) -> PresentationFrame {
        self.frame
    }

    /// The current autonomous intent (test seam; presentation-only).
    #[must_use]
    pub fn current_intent(&self) -> IdleIntent {
        self.intent
    }

    /// Shows the eating pose for a short presentation-only reaction after an
    /// authoritative feed. Never mutates care state.
    pub fn react_to_feed(&mut self, now: Duration) {
        self.reaction = Some((PetPose::Eating, now + Duration::from_millis(2_000)));
    }

    /// Shows the petted pose for a short presentation-only reaction after an
    /// authoritative pet. Never mutates care state.
    pub fn react_to_pet(&mut self, now: Duration) {
        self.reaction = Some((PetPose::Petted, now + Duration::from_millis(1_500)));
    }

    /// Advances the presentation clock to `now` and returns the current frame.
    ///
    /// `area` is the room rectangle; `snapshot` is the latest authoritative
    /// state (a missing snapshot behaves as calm, neutral state).
    #[must_use]
    pub fn tick(
        &mut self,
        now: Duration,
        snapshot: Option<&SimulationSnapshot>,
        area: Rect,
    ) -> PresentationFrame {
        if now < self.intent_started + self.intent_duration {
            self.advance_within_intent(now, area);
            return self.apply_reaction(now);
        }
        self.choose_intent(now, snapshot, area);
        self.apply_reaction(now)
    }

    fn apply_reaction(&mut self, now: Duration) -> PresentationFrame {
        if let Some((pose, until)) = self.reaction {
            if now >= until {
                self.reaction = None;
            } else if self.frame.pose != PetPose::Sleep {
                self.frame.pose = pose;
            }
        }
        self.frame
    }

    fn choose_intent(&mut self, now: Duration, snapshot: Option<&SimulationSnapshot>, area: Rect) {
        self.home = home_for(area);
        let activity = snapshot.map(presentation_activity);
        let needs = snapshot.map(|snapshot| snapshot.needs);
        let napping = snapshot.is_some_and(has_authoritative_nap);
        let generic_sleeping = snapshot.is_some_and(|snapshot| {
            snapshot.behavior == PetBehavior::Sleeping && !has_authoritative_nap(snapshot)
        });

        if napping {
            self.intent = IdleIntent::Sit;
            self.intent_duration = Duration::from_secs(60);
            self.frame.pose = PetPose::Sleep;
            self.frame.offset = (0, 0);
            self.target = self.home;
            return;
        }

        let roll = self.rng.next_u64() % 100;

        // Short, broad activity reactions take priority but remain occasional.
        match activity {
            Some(PresentationActivity::Success) if roll < 40 => {
                self.start_intent(
                    now,
                    IdleIntent::Celebrate,
                    Duration::from_millis(2_000),
                    area,
                );
                return;
            }
            Some(PresentationActivity::Failure) if roll < 40 => {
                self.start_intent(now, IdleIntent::Worry, Duration::from_millis(2_200), area);
                return;
            }
            Some(
                PresentationActivity::Thinking
                | PresentationActivity::Working
                | PresentationActivity::WaitingOrBlocked,
            ) if roll < 20 => {
                self.start_intent(
                    now,
                    IdleIntent::WatchCodex,
                    Duration::from_millis(2_600),
                    area,
                );
                return;
            }
            _ => {}
        }

        // Need influence: express without repairing. Needs are on the domain
        // 0..100 scale and hunger is inverted (0 = full, 100 = starving). The
        // bias is probabilistic so need-driven behavior never becomes an
        // exclusive loop that stops the pet from wandering.
        if generic_sleeping && roll < 30 {
            self.start_intent(now, IdleIntent::Yawn, Duration::from_millis(1_800), area);
            return;
        }
        if let Some(needs) = needs {
            let food_roll = self.rng.next_u64() % 100;
            let bed_roll = self.rng.next_u64() % 100;
            let lonely_roll = self.rng.next_u64() % 100;
            if needs.hunger() >= 65.0 && food_roll < 60 {
                self.start_intent(
                    now,
                    IdleIntent::Inspect(RoomObject::Food),
                    Duration::from_millis(3_200),
                    area,
                );
                return;
            }
            if needs.energy() <= 25.0 && bed_roll < 55 {
                self.start_intent(
                    now,
                    IdleIntent::Inspect(RoomObject::Bed),
                    Duration::from_millis(3_200),
                    area,
                );
                return;
            }
            if needs.happiness() <= 25.0 && lonely_roll < 50 {
                self.start_intent(
                    now,
                    IdleIntent::WatchCodex,
                    Duration::from_millis(2_800),
                    area,
                );
                return;
            }
        }

        // Calm life: wander, sit, inspect, look out the window, yawn, blink.
        let calm = match roll % 10 {
            0 => IdleIntent::Sit,
            1 => IdleIntent::LookOutWindow,
            2 => IdleIntent::Yawn,
            3 => IdleIntent::Inspect(RoomObject::Plants),
            4 => IdleIntent::Inspect(RoomObject::Shelf),
            5 => IdleIntent::Inspect(RoomObject::Laptop),
            6 => IdleIntent::Sit,
            _ => IdleIntent::Wander(random_point(&mut self.rng, self.home, area)),
        };
        let duration = match calm {
            IdleIntent::Wander(_) => Duration::from_millis(self.rng.range(3_000, 8_000)),
            IdleIntent::Sit => Duration::from_millis(self.rng.range(2_000, 5_000)),
            IdleIntent::Yawn => Duration::from_millis(self.rng.range(1_200, 2_000)),
            _ => Duration::from_millis(self.rng.range(2_000, 4_000)),
        };
        self.start_intent(now, calm, duration, area);
    }

    fn start_intent(&mut self, now: Duration, intent: IdleIntent, duration: Duration, area: Rect) {
        self.intent = intent;
        self.intent_started = now;
        self.intent_duration = duration;
        self.phase = 0;
        let target = match intent {
            IdleIntent::Wander(target) => target,
            IdleIntent::Inspect(object) => anchor_for(object, area),
            _ => self.target,
        };
        self.frame.pose = base_pose(intent);
        self.target = target;
    }

    fn advance_within_intent(&mut self, now: Duration, area: Rect) {
        if self.frame.pose == PetPose::Sleep {
            return;
        }
        let walking = matches!(self.intent, IdleIntent::Wander(_));
        let inspecting = matches!(self.intent, IdleIntent::Inspect(_));

        // Wander and inspect step toward their target; walking alternates A/B.
        if walking || inspecting {
            self.phase = self.phase.wrapping_add(1);
            let target = match self.intent {
                IdleIntent::Wander(target) => target,
                _ => self.target,
            };
            let (dx, dy) = step_toward(self.frame.offset, target, self.home, 1);
            let candidate = (self.frame.offset.0 + dx, self.frame.offset.1 + dy);
            self.frame.offset = if candidate_overlaps_reserved_care(candidate, self.home, area) {
                self.frame.offset
            } else {
                clamp_offset(candidate, area)
            };
        }

        let base = if matches!(self.intent, IdleIntent::Wander(_)) {
            if self.phase.is_multiple_of(2) {
                PetPose::WalkA
            } else {
                PetPose::WalkB
            }
        } else {
            base_pose(self.intent)
        };

        // Occasional blink during dwell poses keeps the pet visibly animated
        // even while standing still or inspecting furniture.
        if now >= self.next_blink {
            self.next_blink = now + Duration::from_millis(self.rng.range(2_000, 6_000));
            self.blink_until = now + Duration::from_millis(300);
        }
        let blinking = matches!(
            base,
            PetPose::Idle | PetPose::Sit | PetPose::Curious | PetPose::Doze
        ) && now < self.blink_until;
        self.frame.pose = if blinking { PetPose::Blink } else { base };
    }
}

/// The base pose for an autonomous intent (walk A/B alternates while moving).
fn base_pose(intent: IdleIntent) -> PetPose {
    match intent {
        IdleIntent::Wander(_) => PetPose::WalkA,
        IdleIntent::Sit => PetPose::Sit,
        IdleIntent::Inspect(_) | IdleIntent::LookOutWindow | IdleIntent::WatchCodex => {
            PetPose::Curious
        }
        IdleIntent::Yawn => PetPose::Yawn,
        IdleIntent::Celebrate => PetPose::Happy,
        IdleIntent::Worry => PetPose::Upset,
    }
}

/// Room-relative anchor for an autonomous inspection target.
fn anchor_for(object: RoomObject, area: Rect) -> Position {
    match object {
        RoomObject::Window => Position::new(area.width.saturating_sub(2), 1),
        RoomObject::Laptop => {
            Position::new((area.width / 3).max(10), area.height.saturating_sub(8))
        }
        RoomObject::Shelf => Position::new(area.width.saturating_sub(4), 2),
        RoomObject::Plants => Position::new(2, area.height.saturating_sub(5)),
        RoomObject::Bed => {
            Position::new(area.width.saturating_sub(12), area.height.saturating_sub(4))
        }
        RoomObject::Food => Position::new(8, area.height.saturating_sub(3)),
    }
}

/// Moves `offset` toward the absolute room-relative `target` (converted to a
/// goal offset against the layout `home`) by at most `step` cells per axis.
fn step_toward(offset: (i16, i16), target: Position, home: Position, step: i16) -> (i16, i16) {
    let goal_x =
        i16::try_from(target.x).unwrap_or(i16::MAX) - i16::try_from(home.x).unwrap_or(i16::MAX);
    let goal_y =
        i16::try_from(target.y).unwrap_or(i16::MAX) - i16::try_from(home.y).unwrap_or(i16::MAX);
    (
        (goal_x - offset.0).clamp(-step, step),
        (goal_y - offset.1).clamp(-step, step),
    )
}

/// Clamps a wander offset so the pet stays inside the room's lower lane.
fn clamp_offset(offset: (i16, i16), area: Rect) -> (i16, i16) {
    let max_x = i16::try_from(area.width.saturating_sub(12))
        .unwrap_or(4)
        .max(4);
    let max_y = i16::try_from(area.height.saturating_sub(7))
        .unwrap_or(0)
        .max(0);
    let y_low = -2i16;
    let y_high = max_y.saturating_sub(4).max(y_low);
    (offset.0.clamp(-max_x, max_x), offset.1.clamp(y_low, y_high))
}

fn candidate_overlaps_reserved_care(offset: (i16, i16), home: Position, area: Rect) -> bool {
    if area.height < 14 || area.width < 80 {
        return false;
    }
    let candidate = offset_rect(
        absolute_room_rect(area, Rect::new(home.x, home.y, 18, 7)),
        offset,
        area,
    );
    candidate.intersects(wide_full_care_zone(area))
}

fn random_point(rng: &mut SplitMix64, home: Position, area: Rect) -> Position {
    let min_x = 2u16.max(home.x.saturating_sub(6));
    let max_x = area.width.saturating_sub(12).max(min_x);
    let min_y = home.y.saturating_sub(1);
    let max_y = area.height.saturating_sub(7).max(min_y);
    let x = rng.range(u64::from(min_x), u64::from(max_x).max(u64::from(min_x) + 1)) as u16;
    let y = rng.range(u64::from(min_y), u64::from(max_y).max(u64::from(min_y) + 1)) as u16;
    Position::new(x, y)
}

/// Mirrors the room layout home pet position so wander offsets convert
/// absolute room targets into relative offsets in the same space as
/// `room_geometry`. The three layouts use the same anchors as room.rs.
fn home_for(area: Rect) -> Position {
    if area.height >= 14 {
        let bed_x = if area.width >= 80 {
            area.width.saturating_sub(24)
        } else if area.width <= 64 {
            area.width.saturating_sub(20).max(4)
        } else {
            area.width.saturating_sub(28).max(4)
        };
        if area.width >= 80 {
            wide_full_pet_home(area)
        } else {
            Position::new(bed_x.saturating_sub(18), 4)
        }
    } else if area.height >= 7 {
        let pet_x = if area.width >= 80 {
            2
        } else {
            area.width.saturating_sub(52).max(2)
        };
        Position::new(pet_x, 2)
    } else {
        Position::new(0, 0)
    }
}

/// Dependency-free deterministic splitmix64 PRNG so presentation tests are
/// reproducible without adding a rand dependency.
#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Returns a value in `[low, high)`.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        if high <= low {
            return low;
        }
        low + (self.next_u64() % (high - low))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_positions_match_room_geometry_anchors() {
        assert_eq!(home_for(Rect::new(0, 0, 120, 14)), Position::new(76, 4));
        assert_eq!(home_for(Rect::new(0, 0, 100, 14)), Position::new(56, 4));
        assert_eq!(home_for(Rect::new(0, 0, 80, 14)), Position::new(36, 4));
        assert_eq!(home_for(Rect::new(0, 0, 70, 14)), Position::new(24, 4));
        assert_eq!(home_for(Rect::new(0, 0, 120, 7)), Position::new(2, 2));
        assert_eq!(home_for(Rect::new(0, 0, 70, 7)), Position::new(18, 2));
        assert_eq!(home_for(Rect::new(0, 0, 120, 3)), Position::new(0, 0));
    }
}

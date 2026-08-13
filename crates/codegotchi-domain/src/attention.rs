use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MIN_INCIDENT_DELAY_MS: u64 = 180_000;
pub const MAX_INCIDENT_DELAY_MS: u64 = 300_000;
pub const MAX_CATCH_UP_INCIDENTS: usize = 5;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PetDemandKind {
    Affection,
    Snack,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetDemand {
    id: Uuid,
    kind: PetDemandKind,
    created_at: DateTime<Utc>,
}

impl PetDemand {
    pub fn new(id: Uuid, kind: PetDemandKind, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            kind,
            created_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn kind(&self) -> PetDemandKind {
        self.kind
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[allow(dead_code)]
    pub(crate) fn shift_created_at(&mut self, shift: Duration) {
        self.created_at += shift;
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionIncidentKind {
    Affection,
    Poop,
    Snack,
}

fn hash_u64(pet_id: Uuid, namespace: &str, index: u64) -> u64 {
    let name = format!("attention:{namespace}:{index}");
    let hash = Uuid::new_v5(&pet_id, name.as_bytes());
    u64::from_be_bytes(hash.as_bytes()[0..8].try_into().expect("uuid prefix"))
}

pub fn incident_delay_ms(pet_id: Uuid, sequence: u64) -> u64 {
    let span = MAX_INCIDENT_DELAY_MS - MIN_INCIDENT_DELAY_MS + 1;
    MIN_INCIDENT_DELAY_MS + hash_u64(pet_id, "delay", sequence) % span
}

pub fn incident_kind(pet_id: Uuid, sequence: u64) -> AttentionIncidentKind {
    const SHUFFLE_BAGS: [[AttentionIncidentKind; 3]; 6] = [
        [
            AttentionIncidentKind::Affection,
            AttentionIncidentKind::Snack,
            AttentionIncidentKind::Poop,
        ],
        [
            AttentionIncidentKind::Affection,
            AttentionIncidentKind::Poop,
            AttentionIncidentKind::Snack,
        ],
        [
            AttentionIncidentKind::Snack,
            AttentionIncidentKind::Affection,
            AttentionIncidentKind::Poop,
        ],
        [
            AttentionIncidentKind::Snack,
            AttentionIncidentKind::Poop,
            AttentionIncidentKind::Affection,
        ],
        [
            AttentionIncidentKind::Poop,
            AttentionIncidentKind::Affection,
            AttentionIncidentKind::Snack,
        ],
        [
            AttentionIncidentKind::Poop,
            AttentionIncidentKind::Snack,
            AttentionIncidentKind::Affection,
        ],
    ];

    let bag = (hash_u64(pet_id, "bag", sequence / 3) % SHUFFLE_BAGS.len() as u64) as usize;
    SHUFFLE_BAGS[bag][(sequence % 3) as usize]
}

pub fn incident_id(pet_id: Uuid, sequence: u64, kind: AttentionIncidentKind) -> Uuid {
    let kind_name = match kind {
        AttentionIncidentKind::Affection => "affection",
        AttentionIncidentKind::Snack => "snack",
        AttentionIncidentKind::Poop => "poop",
    };
    let name = format!("attention:{kind_name}:{sequence}");
    Uuid::new_v5(&pet_id, name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        AttentionIncidentKind, MAX_INCIDENT_DELAY_MS, MIN_INCIDENT_DELAY_MS, incident_delay_ms,
        incident_id, incident_kind,
    };
    use uuid::Uuid;

    #[test]
    fn delay_is_deterministic_and_inclusive() {
        let pet_id = Uuid::from_u128(42);
        for sequence in 0..10_000 {
            let delay = incident_delay_ms(pet_id, sequence);
            assert!((MIN_INCIDENT_DELAY_MS..=MAX_INCIDENT_DELAY_MS).contains(&delay));
            assert_eq!(delay, incident_delay_ms(pet_id, sequence));
        }
    }

    #[test]
    fn every_shuffle_bag_contains_each_kind_once() {
        let pet_id = Uuid::from_u128(42);
        for bag in 0..100 {
            let mut kinds = [
                incident_kind(pet_id, bag * 3),
                incident_kind(pet_id, bag * 3 + 1),
                incident_kind(pet_id, bag * 3 + 2),
            ];
            kinds.sort();
            assert_eq!(
                kinds,
                [
                    AttentionIncidentKind::Affection,
                    AttentionIncidentKind::Poop,
                    AttentionIncidentKind::Snack,
                ]
            );
        }
    }

    #[test]
    fn incident_ids_are_stable_and_kind_namespaces_do_not_collide() {
        let pet_id = Uuid::from_u128(42);
        let affection = incident_id(pet_id, 7, AttentionIncidentKind::Affection);
        let poop = incident_id(pet_id, 7, AttentionIncidentKind::Poop);
        assert_eq!(
            affection,
            incident_id(pet_id, 7, AttentionIncidentKind::Affection)
        );
        assert_ne!(affection, poop);
    }
}

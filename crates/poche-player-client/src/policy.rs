// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic policies over exact advertised action sets.

use poche_protocol::CommandPayload;

use crate::{AdvertisedAction, DeviceObservation};

/// Which advertised capabilities a policy is allowed to consider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyScope {
    AllAdvertised,
    PlayerGameActions,
}

/// Small deterministic policies suitable for smoke tests and baseline agents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvertisedActionPolicy {
    FirstLegal,
    SeededRandom { seed: u64 },
}

impl AdvertisedActionPolicy {
    /// Select only from actions advertised for this exact observation.
    #[must_use]
    pub fn select(
        self,
        observation: &DeviceObservation,
        scope: PolicyScope,
    ) -> Option<&AdvertisedAction> {
        let eligible = observation
            .actions
            .iter()
            .filter(|action| in_scope(action, scope))
            .collect::<Vec<_>>();
        match self {
            Self::FirstLegal => eligible.first().copied(),
            Self::SeededRandom { seed } => {
                if eligible.is_empty() {
                    return None;
                }
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"poche.advertised-action-policy.v1\0");
                hasher.update(&seed.to_le_bytes());
                hasher.update(&observation.projection.current_revision.to_le_bytes());
                hasher.update(&observation.projection_hash.0);
                hasher.update(observation.projection.principal_id.as_str().as_bytes());
                for action in &eligible {
                    hasher.update(&(action.id.len() as u64).to_le_bytes());
                    hasher.update(action.id.as_bytes());
                }
                let digest = hasher.finalize();
                let sample = u16::from_le_bytes([digest.as_bytes()[0], digest.as_bytes()[1]]);
                let index = usize::from(sample) % eligible.len();
                eligible.get(index).copied()
            }
        }
    }
}

fn in_scope(action: &AdvertisedAction, scope: PolicyScope) -> bool {
    match scope {
        PolicyScope::AllAdvertised => true,
        PolicyScope::PlayerGameActions => {
            matches!(action.payload, CommandPayload::GameAction { .. })
        }
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CorrelationId, EventId, GameActionWire, PrincipalId, ProjectionEnvelope, ProjectionId,
        ProjectionPayload, RoomId, RoomPhase, SIGNATURE_DOMAIN_V1, SemanticHash,
        SignatureAlgorithm, SignatureBytes, SignatureMetadata,
    };

    use super::*;

    fn observation() -> DeviceObservation {
        DeviceObservation {
            projection: ProjectionEnvelope {
                protocol_version: 1,
                room_id: RoomId::new("policy-room").unwrap(),
                session_epoch: 1,
                projection_id: ProjectionId::new("policy-projection").unwrap(),
                principal_id: PrincipalId::new("alice").unwrap(),
                current_revision: 7,
                projection_epoch: 1,
                correlation_id: CorrelationId::new("policy-correlation").unwrap(),
                causation_id: EventId::new("policy-event").unwrap(),
                payload: ProjectionPayload {
                    phase: RoomPhase::Running,
                    members: Vec::new(),
                    public_game_state: None,
                    own_hand: None,
                    granted_hands: Vec::new(),
                    public_history: Vec::new(),
                },
                signature: SignatureMetadata {
                    domain_version: SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: PrincipalId::new("alice").unwrap(),
                    signature: SignatureBytes::new("0".repeat(128)).unwrap(),
                },
            },
            projection_hash: SemanticHash([7; 32]),
            actions: vec![
                AdvertisedAction {
                    id: "close-room".to_owned(),
                    label: "Close room".to_owned(),
                    payload: CommandPayload::CloseRoom,
                },
                AdvertisedAction {
                    id: "game-bid-0".to_owned(),
                    label: "Bid 0 Tricks".to_owned(),
                    payload: CommandPayload::GameAction {
                        action: GameActionWire::Bid { tricks: 0 },
                    },
                },
                AdvertisedAction {
                    id: "game-bid-1".to_owned(),
                    label: "Bid 1 Trick".to_owned(),
                    payload: CommandPayload::GameAction {
                        action: GameActionWire::Bid { tricks: 1 },
                    },
                },
            ],
            action_templates: Vec::new(),
            chat_tail: Vec::new(),
            capture_providers: Vec::new(),
            physical_hands: Vec::new(),
            physical_public: Vec::new(),
        }
    }

    #[test]
    fn game_agent_never_selects_a_room_control() {
        let observation = observation();
        let selected = AdvertisedActionPolicy::FirstLegal
            .select(&observation, PolicyScope::PlayerGameActions)
            .unwrap();
        assert_eq!(selected.id, "game-bid-0");
    }

    #[test]
    fn seeded_policy_is_repeatable_and_remains_in_scope() {
        let observation = observation();
        let first = AdvertisedActionPolicy::SeededRandom { seed: 41 }
            .select(&observation, PolicyScope::PlayerGameActions)
            .unwrap();
        let second = AdvertisedActionPolicy::SeededRandom { seed: 41 }
            .select(&observation, PolicyScope::PlayerGameActions)
            .unwrap();
        assert_eq!(first.id, second.id);
        assert!(matches!(first.payload, CommandPayload::GameAction { .. }));
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::{
    ActionIndex, DecisionView, Policy, PolicyError, RlEnvironmentError, RlSpec, action_label,
};

/// One completed turn-based transition. The actor's action is paired with
/// that same seat's next decision observation or the terminal boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatTransitionRecord {
    pub seat: usize,
    pub decision_ordinal: u64,
    pub observation_hash: String,
    pub action: usize,
    pub action_label: String,
    pub reward_at_next_observation: f32,
    pub next_observation_hash: String,
    pub terminal: bool,
    pub decision_time_distance: u32,
}

/// Secret-free, compact episode replay. It contains semantic hashes and policy
/// inputs/outputs, never hidden cards or chance deck order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeTranscript {
    pub schema_version: u16,
    pub spec_id: String,
    pub spec_hash: String,
    pub reward_id: String,
    pub seed: u64,
    pub policy: String,
    pub seat_policies: [String; 2],
    pub transitions: Vec<SeatTransitionRecord>,
    pub round_points: Vec<[u16; 2]>,
    pub final_scores: [u16; 2],
    pub score_differential_seat0: i32,
    pub pot_cents: u32,
    pub winners: [bool; 2],
    pub exact_bid_count: [u32; 2],
    pub illegal_action_count: u64,
    pub game_length_decisions: u64,
}

impl EpisodeTranscript {
    /// Encode the compact canonical replay.
    ///
    /// # Errors
    ///
    /// Returns a serialization error for an unencodable value.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Hash the canonical compact replay.
    ///
    /// # Errors
    ///
    /// Returns a serialization error for an unencodable value.
    pub fn semantic_hash(&self) -> Result<String, serde_json::Error> {
        self.canonical_json()
            .map(|text| blake3::hash(text.as_bytes()).to_hex().to_string())
    }

    /// Inspectable protocol-style NDJSON: one header, ordered transitions, one
    /// terminal summary. This serialization is kept out of rollout hot paths.
    ///
    /// # Errors
    ///
    /// Returns a serialization error for an unencodable value.
    pub fn ndjson(&self) -> Result<String, serde_json::Error> {
        let mut lines = Vec::with_capacity(self.transitions.len() + 2);
        lines.push(serde_json::to_string(&serde_json::json!({
            "kind": "poche_rl_episode",
            "schema_version": self.schema_version,
            "spec_id": self.spec_id,
            "spec_hash": self.spec_hash,
            "reward_id": self.reward_id,
            "seed": self.seed,
            "policy": self.policy,
            "seat_policies": self.seat_policies,
        }))?);
        for transition in &self.transitions {
            lines.push(serde_json::to_string(&serde_json::json!({
                "kind": "seat_transition",
                "data": transition,
            }))?);
        }
        lines.push(serde_json::to_string(&serde_json::json!({
            "kind": "episode_terminal",
            "final_scores": self.final_scores,
            "score_differential_seat0": self.score_differential_seat0,
            "round_points": self.round_points,
            "pot_cents": self.pot_cents,
            "winners": self.winners,
            "exact_bid_count": self.exact_bid_count,
            "illegal_action_count": self.illegal_action_count,
            "game_length_decisions": self.game_length_decisions,
        }))?);
        Ok(format!("{}\n", lines.join("\n")))
    }
}

/// Episode execution failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeError {
    Environment(RlEnvironmentError),
    Policy(PolicyError),
    Manifest,
    MissingTerminal,
}

impl From<RlEnvironmentError> for EpisodeError {
    fn from(value: RlEnvironmentError) -> Self {
        Self::Environment(value)
    }
}

impl From<PolicyError> for EpisodeError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}

#[derive(Clone, Debug)]
struct PendingDecision {
    decision_ordinal: u64,
    observation_hash: String,
    action: usize,
    action_label: String,
    reward: f32,
    distance: u32,
}

/// Run a complete deterministic full-rule episode with a policy controlling
/// both seats. Serialization happens only after the rollout completes.
///
/// # Errors
///
/// Returns a typed manifest, environment, policy, or terminal-boundary error.
pub fn run_episode(seed: u64, policy: &mut impl Policy) -> Result<EpisodeTranscript, EpisodeError> {
    let name = policy.name().to_owned();
    run_episode_dispatch(seed, [name.clone(), name], |_, decision| {
        policy.select(decision)
    })
}

/// Run one episode with independently named policies assigned to each seat.
///
/// # Errors
///
/// Returns a typed manifest, environment, policy, or terminal-boundary error.
pub fn run_episode_with_seat_policies(
    seed: u64,
    seat0: &mut dyn Policy,
    seat1: &mut dyn Policy,
) -> Result<EpisodeTranscript, EpisodeError> {
    let names = [seat0.name().to_owned(), seat1.name().to_owned()];
    run_episode_dispatch(seed, names, |seat, decision| {
        if seat == 0 {
            seat0.select(decision)
        } else {
            seat1.select(decision)
        }
    })
}

fn run_episode_dispatch(
    seed: u64,
    seat_policies: [String; 2],
    mut select: impl FnMut(usize, &DecisionView) -> Result<ActionIndex, PolicyError>,
) -> Result<EpisodeTranscript, EpisodeError> {
    let spec = RlSpec::poche_2p_v1();
    spec.validate().map_err(|_| EpisodeError::Manifest)?;
    let spec_hash = spec.semantic_hash().map_err(|_| EpisodeError::Manifest)?;
    let mut env = crate::PocheRlEnv::reset(seed)?;
    let mut pending: [Option<PendingDecision>; 2] = [None, None];
    let mut transitions = Vec::new();
    let mut round_points = Vec::new();
    let mut exact_bid_count = [0_u32; 2];
    let terminal_outcome = loop {
        let decision = env.decision().ok_or(EpisodeError::MissingTerminal)?;
        let seat = decision.seat.index();
        if let Some(completed) = pending[seat].take() {
            transitions.push(complete_pending(
                seat,
                completed,
                decision.observation_hash.clone(),
                false,
            ));
        }
        let action = select(seat, &decision)?;
        pending[seat] = Some(PendingDecision {
            decision_ordinal: env.decision_count(),
            observation_hash: decision.observation_hash,
            action: action.get(),
            action_label: action_label(action),
            reward: 0.0,
            distance: 0,
        });
        let step = env.step(action)?;
        for (index, item) in pending.iter_mut().enumerate() {
            if let Some(item) = item {
                item.reward += step.instant_rewards[index];
                item.distance = item.distance.saturating_add(1);
            }
        }
        if let Some(points) = step.round_points {
            round_points.push(points);
        }
        if let Some(exact) = step.exact_bids {
            for (index, was_exact) in exact.into_iter().enumerate() {
                exact_bid_count[index] += u32::from(was_exact);
            }
        }
        if let Some(outcome) = step.terminal_outcome {
            for (seat, item) in pending.iter_mut().enumerate() {
                if let Some(completed) = item.take() {
                    transitions.push(complete_pending(
                        seat,
                        completed,
                        "terminal:poche-2p-v1".to_owned(),
                        true,
                    ));
                }
            }
            break outcome;
        }
    };
    transitions.sort_by_key(|record| record.decision_ordinal);
    let outcome = terminal_outcome;
    let policy_name = if seat_policies[0] == seat_policies[1] {
        seat_policies[0].clone()
    } else {
        format!("seat0={} vs seat1={}", seat_policies[0], seat_policies[1])
    };
    Ok(EpisodeTranscript {
        schema_version: 1,
        spec_id: spec.spec_id,
        spec_hash,
        reward_id: spec.reward_id,
        seed,
        policy: policy_name,
        seat_policies,
        transitions,
        round_points,
        final_scores: outcome.scores,
        score_differential_seat0: i32::from(outcome.scores[0]) - i32::from(outcome.scores[1]),
        pot_cents: outcome.pot_cents,
        winners: outcome.winners,
        exact_bid_count,
        illegal_action_count: env.illegal_action_count(),
        game_length_decisions: env.decision_count(),
    })
}

fn complete_pending(
    seat: usize,
    pending: PendingDecision,
    next_observation_hash: String,
    terminal: bool,
) -> SeatTransitionRecord {
    SeatTransitionRecord {
        seat,
        decision_ordinal: pending.decision_ordinal,
        observation_hash: pending.observation_hash,
        action: pending.action,
        action_label: pending.action_label,
        reward_at_next_observation: pending.reward,
        next_observation_hash,
        terminal,
        decision_time_distance: pending.distance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LegalRandomPolicy;

    #[test]
    fn transcript_replays_by_seed_and_preserves_score_as_primary_metric() {
        let first = run_episode(0x5eed, &mut LegalRandomPolicy::new(99)).unwrap();
        let replay = run_episode(0x5eed, &mut LegalRandomPolicy::new(99)).unwrap();
        assert_eq!(first, replay);
        assert_eq!(
            first.semantic_hash().unwrap(),
            replay.semantic_hash().unwrap()
        );
        assert_eq!(first.round_points.len(), 13);
        assert_eq!(
            first.final_scores,
            first
                .round_points
                .iter()
                .fold([0_u16; 2], |mut total, round| {
                    total[0] += round[0];
                    total[1] += round[1];
                    total
                })
        );
        assert_eq!(first.illegal_action_count, 0);
        assert!(
            first
                .transitions
                .iter()
                .all(|item| item.decision_time_distance > 0)
        );
        let ndjson = first.ndjson().unwrap();
        assert!(ndjson.ends_with('\n'));
        assert!(!ndjson.contains("private_hand"));
        assert!(!ndjson.contains("deck"));
    }
}

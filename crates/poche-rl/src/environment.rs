// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_environment::{
    EnvironmentAction, GameEnvironment, OracleChanceAction, OracleEnvironment, OracleGameOutcome,
    OraclePlayerAction, TerminalStatus, TurnOwner,
};
use poche_oracle_rust::{Card, Observation, PhaseTag, Rank, RuleViolation, Seat, Suit, Turn};

use crate::{ACTION_COUNT, BID_OFFSET, CARD_OFFSET, OBSERVATION_SIZE, RlSpec};

type Env = OracleEnvironment<2>;

/// Stable index in the fixed `poche-2p-v1` policy vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionIndex(usize);

impl ActionIndex {
    /// Validate one action-vocabulary index.
    ///
    /// # Errors
    ///
    /// Returns [`RlEnvironmentError::InvalidActionIndex`] outside the fixed
    /// vocabulary.
    pub const fn new(index: usize) -> Result<Self, RlEnvironmentError> {
        if index < ACTION_COUNT {
            Ok(Self(index))
        } else {
            Err(RlEnvironmentError::InvalidActionIndex)
        }
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Explicit production handling for masked actions. Both variants reject; the
/// latter additionally increments a diagnostic counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IllegalActionPolicy {
    #[default]
    Reject,
    CountAndReject,
}

/// One policy decision input. The observation and mask are exact fixed buffers.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionView {
    pub seat: Seat<2>,
    pub observation: [f32; OBSERVATION_SIZE],
    pub legal_mask: [bool; ACTION_COUNT],
    pub observation_hash: String,
}

/// Result after applying one player decision and auto-advancing only chance
/// and deterministic settlement work.
#[derive(Clone, Debug, PartialEq)]
pub struct RlStep {
    pub actor: Seat<2>,
    pub action: ActionIndex,
    pub instant_rewards: [f32; 2],
    pub round_points: Option<[u16; 2]>,
    pub exact_bids: Option<[bool; 2]>,
    pub next_decision: Option<DecisionView>,
    pub terminal: bool,
    pub terminal_outcome: Option<OracleGameOutcome<2>>,
    pub environment_transitions: u32,
}

/// Redacted RL-boundary failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RlEnvironmentError {
    InvalidActionIndex,
    IllegalAction,
    NoAgentTurn,
    TransitionBufferFull,
    Oracle(RuleViolation),
}

impl From<RuleViolation> for RlEnvironmentError {
    fn from(value: RuleViolation) -> Self {
        Self::Oracle(value)
    }
}

/// One deterministic full-rule two-player environment. It owns public played
/// history because that is viewer-visible episode memory, not hidden game state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PocheRlEnv {
    game: poche_oracle_rust::Game<2>,
    seed: u64,
    deal_ordinal: u32,
    public_played: [bool; 52],
    illegal_action_policy: IllegalActionPolicy,
    illegal_action_count: u64,
    decision_count: u64,
}

impl PocheRlEnv {
    /// Reset to a deterministic episode and auto-deal to the first policy turn.
    ///
    /// # Errors
    ///
    /// Returns a typed oracle/setup failure.
    pub fn reset(seed: u64) -> Result<Self, RlEnvironmentError> {
        Self::reset_with_policy(seed, IllegalActionPolicy::Reject)
    }

    /// Reset with an explicit masked-action diagnostic policy.
    ///
    /// # Errors
    ///
    /// Returns a typed oracle/setup failure.
    pub fn reset_with_policy(
        seed: u64,
        illegal_action_policy: IllegalActionPolicy,
    ) -> Result<Self, RlEnvironmentError> {
        let first_dealer = Seat::new(usize::from((seed & 1) != 0))?;
        let mut value = Self {
            game: Env::initial(first_dealer)?,
            seed,
            deal_ordinal: 0,
            public_played: [false; 52],
            illegal_action_policy,
            illegal_action_count: 0,
            decision_count: 0,
        };
        let _auto = value.auto_advance()?;
        Ok(value)
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn decision_count(&self) -> u64 {
        self.decision_count
    }

    #[must_use]
    pub const fn illegal_action_count(&self) -> u64 {
        self.illegal_action_count
    }

    #[must_use]
    pub const fn game(&self) -> &poche_oracle_rust::Game<2> {
        &self.game
    }

    /// Return the current exact policy input, or `None` after terminal.
    #[must_use]
    pub fn decision(&self) -> Option<DecisionView> {
        let TurnOwner::Agent(seat) = Env::turn(&self.game) else {
            return None;
        };
        let observation = Env::observe(&self.game, seat);
        let encoded = encode_observation(&observation, &self.public_played);
        Some(DecisionView {
            seat,
            observation_hash: observation_hash(&encoded),
            observation: encoded,
            legal_mask: legal_action_mask(&self.game),
        })
    }

    /// Apply one exact legal action and auto-advance chance/settlement.
    ///
    /// # Errors
    ///
    /// Returns an invalid/masked action, missing agent turn, or typed reducer
    /// failure. Masked actions are never remapped.
    pub fn step(&mut self, action: ActionIndex) -> Result<RlStep, RlEnvironmentError> {
        let Some(decision) = self.decision() else {
            return Err(RlEnvironmentError::NoAgentTurn);
        };
        if !decision.legal_mask[action.get()] {
            if self.illegal_action_policy == IllegalActionPolicy::CountAndReject {
                self.illegal_action_count = self.illegal_action_count.saturating_add(1);
            }
            return Err(RlEnvironmentError::IllegalAction);
        }
        let player_action = action_to_player_action(action, decision.seat)?;
        if let OraclePlayerAction::Play { card, .. } = player_action {
            self.public_played[card_index(card)] = true;
        }
        self.game = Env::transition(&self.game, EnvironmentAction::Player(player_action))?.state;
        self.decision_count = self.decision_count.saturating_add(1);
        let auto = self.auto_advance()?;
        let terminal_outcome = match Env::terminal_status(&self.game) {
            TerminalStatus::Ongoing => None,
            TerminalStatus::Finished(outcome) => Some(outcome),
        };
        Ok(RlStep {
            actor: decision.seat,
            action,
            instant_rewards: auto.rewards,
            round_points: auto.round_points,
            exact_bids: auto.exact_bids,
            next_decision: self.decision(),
            terminal: terminal_outcome.is_some(),
            terminal_outcome,
            environment_transitions: auto.transitions,
        })
    }

    fn auto_advance(&mut self) -> Result<AutoAdvance, RlEnvironmentError> {
        let mut result = AutoAdvance::default();
        loop {
            match Env::turn(&self.game) {
                TurnOwner::Chance => {
                    let chance = OracleChanceAction::seeded(self.seed, self.deal_ordinal);
                    self.deal_ordinal = self.deal_ordinal.saturating_add(1);
                    self.game =
                        Env::transition(&self.game, EnvironmentAction::Chance(chance))?.state;
                    result.transitions = result.transitions.saturating_add(1);
                }
                TurnOwner::Environment => {
                    let transition = Env::transition(&self.game, EnvironmentAction::Settle)?;
                    if let Some(scores) = transition.round_scores {
                        let mut points = [0_u16; 2];
                        let mut exact = [false; 2];
                        for score in scores {
                            let index = score.agent.index();
                            points[index] = score.score.points;
                            exact[index] = matches!(
                                score.score.outcome,
                                poche_oracle_rust::BidOutcome::Exact
                                    | poche_oracle_rust::BidOutcome::AllTricks
                            );
                            result.rewards[index] += f32::from(score.score.points);
                        }
                        result.round_points = Some(points);
                        result.exact_bids = Some(exact);
                    }
                    self.game = transition.state;
                    result.transitions = result.transitions.saturating_add(1);
                }
                TurnOwner::Agent(_) | TurnOwner::Finished => return Ok(result),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AutoAdvance {
    rewards: [f32; 2],
    round_points: Option<[u16; 2]>,
    exact_bids: Option<[bool; 2]>,
    transitions: u32,
}

/// Encode an already viewer-scoped observation plus explicit public memory.
#[must_use]
pub fn encode_observation(
    observation: &Observation<2>,
    public_played: &[bool; 52],
) -> [f32; OBSERVATION_SIZE] {
    let mut out = [0.0; OBSERVATION_SIZE];
    out[match observation.phase {
        PhaseTag::AwaitingDeal => 0,
        PhaseTag::Bidding => 1,
        PhaseTag::Playing => 2,
        PhaseTag::Scoring => 3,
        PhaseTag::Finished => 4,
    }] = 1.0;
    out[5 + observation.dealer.map_or(
        0,
        |dealer| {
            if dealer == observation.viewer { 1 } else { 2 }
        },
    )] = 1.0;
    out[8 + match observation.actor {
        Turn::Chance => 0,
        Turn::Environment => 1,
        Turn::Finished => 2,
        Turn::Player(actor) if actor == observation.viewer => 3,
        Turn::Player(_) => 4,
    }] = 1.0;
    out[13] = f32::from(u16::try_from(observation.round_index).unwrap_or(u16::MAX)) / 13.0;
    out[14] = f32::from(observation.hand_size) / 7.0;
    for card in observation.private_hand.iter() {
        out[15 + card_index(card)] = 1.0;
    }
    let relative = relative_seats(observation.viewer);
    for (relative_index, seat) in relative.into_iter().enumerate() {
        out[67 + relative_index] = f32::from(observation.hand_counts[seat.index()]) / 7.0;
    }
    out[69 + observation.trump.map_or(0, |card| 1 + card_index(card))] = 1.0;
    for (slot, played) in observation.current_trick.iter().enumerate() {
        let start = 122 + slot * 55;
        out[start] = 1.0;
        out[start + 1 + usize::from(played.player != observation.viewer)] = 1.0;
        out[start + 3 + card_index(played.card)] = 1.0;
    }
    for (relative_index, seat) in relative.into_iter().enumerate() {
        let bid_slot = observation.bids[seat.index()].map_or(0, |bid| 1 + usize::from(bid));
        out[232 + relative_index * 9 + bid_slot] = 1.0;
        out[250 + relative_index] = f32::from(observation.tricks_won[seat.index()]) / 7.0;
        out[252 + relative_index] = f32::from(observation.scores[seat.index()]) / 351.0;
    }
    out[254] = f32::from(u16::try_from(observation.pot_cents).unwrap_or(u16::MAX)) / 512.0;
    for (index, present) in public_played.iter().copied().enumerate() {
        out[255 + index] = f32::from(present);
    }
    out
}

/// Fixed legal-action mask for a typed game state.
#[must_use]
pub fn legal_action_mask(game: &poche_oracle_rust::Game<2>) -> [bool; ACTION_COUNT] {
    let mut mask = [false; ACTION_COUNT];
    for action in Env::legal_actions(game) {
        let index = match action {
            OraclePlayerAction::Bid { tricks, .. } => BID_OFFSET + usize::from(tricks),
            OraclePlayerAction::Play { card, .. } => CARD_OFFSET + card_index(card),
        };
        mask[index] = true;
    }
    mask
}

/// Human label from the immutable manifest vocabulary.
#[must_use]
pub fn action_label(action: ActionIndex) -> String {
    RlSpec::poche_2p_v1().action_vocabulary[action.get()].clone()
}

/// Stable observation digest over exact IEEE-754 little-endian bytes.
#[must_use]
pub fn observation_hash(observation: &[f32; OBSERVATION_SIZE]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-observation-v1\0");
    for value in observation {
        hasher.update(&value.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn action_to_player_action(
    action: ActionIndex,
    actor: Seat<2>,
) -> Result<OraclePlayerAction<2>, RlEnvironmentError> {
    match action.get() {
        index @ BID_OFFSET..CARD_OFFSET => Ok(OraclePlayerAction::Bid {
            player: actor,
            tricks: u8::try_from(index).map_err(|_| RlEnvironmentError::InvalidActionIndex)?,
        }),
        index @ CARD_OFFSET..ACTION_COUNT => Ok(OraclePlayerAction::Play {
            player: actor,
            card: card_from_index(index - CARD_OFFSET),
        }),
        _ => Err(RlEnvironmentError::InvalidActionIndex),
    }
}

#[must_use]
pub(crate) fn card_index(card: Card) -> usize {
    (card.suit as usize) * Rank::ALL.len() + card.rank as usize
}

#[must_use]
pub(crate) fn card_from_index(index: usize) -> Card {
    let suit = Suit::ALL[index / Rank::ALL.len()];
    let rank = Rank::ALL[index % Rank::ALL.len()];
    Card::new(suit, rank)
}

fn relative_seats(viewer: Seat<2>) -> [Seat<2>; 2] {
    [viewer, viewer.left()]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use poche_environment::OracleChanceAction;
    use poche_oracle_rust::{DeckOrder, GameState};

    #[test]
    fn handwritten_initial_tensor_and_mask_match_the_manifest() {
        let env = PocheRlEnv::reset(0x5eed).unwrap();
        let decision = env.decision().unwrap();
        assert_eq!(decision.observation.len(), OBSERVATION_SIZE);
        assert_eq!(decision.legal_mask.len(), ACTION_COUNT);
        assert_eq!(decision.observation[1], 1.0);
        assert_eq!(decision.observation[8 + 3], 1.0);
        assert_eq!(decision.observation[14], 1.0 / 7.0);
        assert_eq!(decision.observation[15..67].iter().sum::<f32>(), 1.0);
        assert_eq!(
            decision.legal_mask[..8]
                .iter()
                .filter(|value| **value)
                .count(),
            2
        );
        assert!(!decision.legal_mask[8..].iter().any(|value| *value));
    }

    #[test]
    fn every_legal_mask_slot_roundtrips_and_illegal_slots_fail() {
        let mut env =
            PocheRlEnv::reset_with_policy(9, IllegalActionPolicy::CountAndReject).unwrap();
        for _ in 0..500 {
            let Some(decision) = env.decision() else {
                break;
            };
            let legal = decision
                .legal_mask
                .iter()
                .position(|allowed| *allowed)
                .unwrap();
            let illegal = decision
                .legal_mask
                .iter()
                .position(|allowed| !*allowed)
                .unwrap();
            assert_eq!(
                env.step(ActionIndex::new(illegal).unwrap()),
                Err(RlEnvironmentError::IllegalAction)
            );
            assert_eq!(env.illegal_action_count(), 1 + env.decision_count());
            env.step(ActionIndex::new(legal).unwrap()).unwrap();
        }
        assert!(env.decision().is_none());
    }

    #[test]
    fn viewer_invisible_hidden_mutation_cannot_change_encoding() {
        let seat = Seat::new(0).unwrap();
        let prepared = Env::initial(Seat::new(1).unwrap()).unwrap();
        let base_cards = Card::standard_deck();
        let base = Env::transition(
            &prepared,
            EnvironmentAction::Chance(OracleChanceAction::explicit(
                DeckOrder::new(base_cards).unwrap(),
            )),
        )
        .unwrap()
        .state;
        let base_view = Env::observe(&base, seat);
        let GameState::Bidding(base_hidden) = base.state() else {
            panic!("dealt")
        };
        let mut found = None;
        'outer: for first in 0..52 {
            for second in (first + 1)..52 {
                let mut cards = base_cards;
                cards.swap(first, second);
                let candidate = Env::transition(
                    &prepared,
                    EnvironmentAction::Chance(OracleChanceAction::explicit(
                        DeckOrder::new(cards).unwrap(),
                    )),
                )
                .unwrap()
                .state;
                let GameState::Bidding(hidden) = candidate.state() else {
                    continue;
                };
                if hidden.hands[0] == base_hidden.hands[0]
                    && hidden.hands[1] != base_hidden.hands[1]
                    && Env::observe(&candidate, seat) == base_view
                {
                    found = Some(candidate);
                    break 'outer;
                }
            }
        }
        let candidate = found.expect("a stock/opponent-only swap exists");
        assert_eq!(
            encode_observation(&base_view, &[false; 52]),
            encode_observation(&Env::observe(&candidate, seat), &[false; 52])
        );
    }

    #[test]
    fn reward_is_zero_between_rounds_and_raw_at_settlement() {
        let mut env = PocheRlEnv::reset(42).unwrap();
        let mut saw_round = false;
        for _ in 0..500 {
            let decision = env.decision().unwrap();
            let action = ActionIndex::new(
                decision
                    .legal_mask
                    .iter()
                    .position(|allowed| *allowed)
                    .unwrap(),
            )
            .unwrap();
            let step = env.step(action).unwrap();
            if let Some(points) = step.round_points {
                assert_eq!(step.instant_rewards, points.map(f32::from));
                assert!(step.instant_rewards.iter().any(|reward| *reward > 0.0));
                saw_round = true;
                break;
            }
            assert_eq!(step.instant_rewards, [0.0; 2]);
        }
        assert!(saw_round);
    }
}

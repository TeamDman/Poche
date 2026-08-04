// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Policy-neutral game-environment boundary shared by simulation and formal
//! checking. Chance is explicit and replayable; hidden state is available only
//! through `observe`; raw round scores are not converted into RL rewards.

use std::array;

use poche_oracle_rust::{
    Action, BidOutcome, Card, DeckOrder, Finished, Game, GameState, Observation, PotDivision,
    RoundScore, RuleViolation, Seat, Turn,
};

/// Rule origin attached to a semantic output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleOrigin {
    /// Stable rule ID from `docs/rules-coverage.md`.
    pub rule_id: &'static str,
}

/// Whose turn it is at the policy-neutral boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOwner<AgentId> {
    /// Environment-provided chance action.
    Chance,
    /// One agent chooses among legal player actions.
    Agent(AgentId),
    /// Deterministic scoring/settlement owned by the environment.
    Environment,
    /// Absorbing terminal state.
    Finished,
}

/// Terminal projection without an RL reward convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalStatus<Outcome> {
    /// The game can continue.
    Ongoing,
    /// The game has a final semantic outcome.
    Finished(Outcome),
}

/// A player-controlled Poche action. Deal and settlement are intentionally not
/// variants of this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OraclePlayerAction<const PLAYERS: usize> {
    /// Announce a fixed bid.
    Bid {
        /// Acting player.
        player: Seat<PLAYERS>,
        /// Whole-number trick bid.
        tricks: u8,
    },
    /// Play one card from the acting player's hand.
    Play {
        /// Acting player.
        player: Seat<PLAYERS>,
        /// Selected card.
        card: Card,
    },
}

/// How an explicit chance action was produced. This metadata is replay input,
/// not part of formal game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChanceProvenance {
    /// A caller supplied an already ordered deck.
    Explicit,
    /// The deterministic seed/round derivation supplied the order.
    Seeded {
        /// Stable replay seed.
        seed: u64,
        /// Zero-based deal ordinal mixed with the seed.
        deal_ordinal: u32,
    },
}

/// Explicit chance action carrying a complete validated deck order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleChanceAction {
    /// Chance result consumed by the game transition.
    pub deck: DeckOrder,
    /// Replay provenance outside formal state.
    pub provenance: ChanceProvenance,
}

impl OracleChanceAction {
    /// Wrap a caller-provided validated deck order.
    #[must_use]
    pub const fn explicit(deck: DeckOrder) -> Self {
        Self {
            deck,
            provenance: ChanceProvenance::Explicit,
        }
    }

    /// Derive a deterministic permutation from `(seed, deal_ordinal)`.
    ///
    /// # Panics
    ///
    /// No panic is reachable for the fixed 52-card array; checked conversions
    /// and deck validation make those assumptions executable.
    #[must_use]
    pub fn seeded(seed: u64, deal_ordinal: u32) -> Self {
        let mut cards = Card::standard_deck();
        let mut state = seed
            ^ u64::from(deal_ordinal).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ 0xd1b5_4a32_d192_ed03;
        for upper in (1..cards.len()).rev() {
            state = splitmix64(state);
            let bound = u64::try_from(upper + 1).expect("deck index fits u64");
            let index = usize::try_from(state % bound).expect("selected deck index fits usize");
            cards.swap(upper, index);
        }
        Self {
            deck: DeckOrder::new(cards).expect("a permutation of the standard deck is valid"),
            provenance: ChanceProvenance::Seeded { seed, deal_ordinal },
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Boundary action with mutually exclusive player, chance, and environment
/// ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentAction<PlayerAction, ChanceAction> {
    /// Agent-selected action.
    Player(PlayerAction),
    /// Explicit chance result.
    Chance(ChanceAction),
    /// Deterministic environment settlement.
    Settle,
}

/// Raw score event at the exact round boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundScoreEvent<AgentId> {
    /// Player receiving the score/payment outcome.
    pub agent: AgentId,
    /// Unprojected rulebook score result.
    pub score: RoundScore,
    /// Scoring-rule origin.
    pub score_origin: RuleOrigin,
    /// Money-rule origin.
    pub money_origin: RuleOrigin,
}

/// Final score and money projections. Score and pot division remain distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleGameOutcome<const PLAYERS: usize> {
    /// Final cumulative rulebook points.
    pub scores: [u16; PLAYERS],
    /// Communal bowl in cents.
    pub pot_cents: u32,
    /// Every player tied at the maximum score.
    pub winners: [bool; PLAYERS],
    /// Quotient and intentionally unassigned indivisible-cent remainder.
    pub pot_division: PotDivision,
}

/// Result of one legal boundary transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionOutcome<State, RoundScores, GameOutcome> {
    /// Successor state.
    pub state: State,
    /// Present exactly when a round was scored.
    pub round_scores: Option<RoundScores>,
    /// Present exactly when the successor is terminal.
    pub game_outcome: Option<GameOutcome>,
}

/// Concrete transition result shape for a [`GameEnvironment`] implementation.
pub type EnvironmentTransition<E> = TransitionOutcome<
    <E as GameEnvironment>::State,
    <E as GameEnvironment>::RoundScores,
    <E as GameEnvironment>::GameOutcome,
>;

/// Minimal interface a simulation, exhaustive checker, or future policy
/// adapter may consume. The policy is never part of transition semantics.
pub trait GameEnvironment {
    /// Complete hidden state.
    type State: Clone + Eq;
    /// Stable acting-agent identifier.
    type AgentId: Copy + Eq;
    /// Viewer-specific information projection.
    type Observation: Clone + Eq;
    /// Player-owned action type.
    type PlayerAction: Clone + Eq;
    /// Chance-owned action type.
    type ChanceAction: Clone + Eq;
    /// Raw per-round score event collection.
    type RoundScores: Clone + Eq;
    /// Final score/money outcome.
    type GameOutcome: Clone + Eq;
    /// Rejected action or invalid state error.
    type Error;

    /// Return the current owner of action selection.
    fn turn(state: &Self::State) -> TurnOwner<Self::AgentId>;

    /// Project hidden state for exactly one viewer.
    fn observe(state: &Self::State, viewer: Self::AgentId) -> Self::Observation;

    /// Enumerate only actions owned by the acting player.
    fn legal_actions(state: &Self::State) -> Vec<Self::PlayerAction>;

    /// Apply a player, chance, or environment action without policy logic.
    ///
    /// # Errors
    ///
    /// Returns the implementation's semantic error for an unavailable or
    /// invalid action.
    fn transition(
        state: &Self::State,
        action: EnvironmentAction<Self::PlayerAction, Self::ChanceAction>,
    ) -> Result<EnvironmentTransition<Self>, Self::Error>;

    /// Return terminal status as a score/money outcome, never as reward.
    fn terminal_status(state: &Self::State) -> TerminalStatus<Self::GameOutcome>;
}

/// Conventional-oracle adapter implementing the shared boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OracleEnvironment<const PLAYERS: usize>;

impl<const PLAYERS: usize> OracleEnvironment<PLAYERS> {
    /// Construct the prepared initial state after dealer selection.
    ///
    /// # Errors
    ///
    /// Returns the conventional oracle's validation error for an unsupported
    /// player count or invalid dealer.
    pub fn initial(first_dealer: Seat<PLAYERS>) -> Result<Game<PLAYERS>, RuleViolation> {
        Game::new(first_dealer)
    }
}

impl<const PLAYERS: usize> GameEnvironment for OracleEnvironment<PLAYERS> {
    type State = Game<PLAYERS>;
    type AgentId = Seat<PLAYERS>;
    type Observation = Observation<PLAYERS>;
    type PlayerAction = OraclePlayerAction<PLAYERS>;
    type ChanceAction = OracleChanceAction;
    type RoundScores = [RoundScoreEvent<Seat<PLAYERS>>; PLAYERS];
    type GameOutcome = OracleGameOutcome<PLAYERS>;
    type Error = RuleViolation;

    fn turn(state: &Self::State) -> TurnOwner<Self::AgentId> {
        match state.turn() {
            Turn::Chance => TurnOwner::Chance,
            Turn::Player(player) => TurnOwner::Agent(player),
            Turn::Environment => TurnOwner::Environment,
            Turn::Finished => TurnOwner::Finished,
        }
    }

    fn observe(state: &Self::State, viewer: Self::AgentId) -> Self::Observation {
        state.observe(viewer)
    }

    fn legal_actions(state: &Self::State) -> Vec<Self::PlayerAction> {
        state
            .legal_player_actions()
            .into_iter()
            .filter_map(|action| match action {
                Action::Bid { player, tricks } => Some(OraclePlayerAction::Bid { player, tricks }),
                Action::Play { player, card } => Some(OraclePlayerAction::Play { player, card }),
                Action::Deal(_) | Action::SettleRound => None,
            })
            .collect()
    }

    fn transition(
        state: &Self::State,
        action: EnvironmentAction<Self::PlayerAction, Self::ChanceAction>,
    ) -> Result<EnvironmentTransition<Self>, Self::Error> {
        let oracle_action = match action {
            EnvironmentAction::Player(OraclePlayerAction::Bid { player, tricks }) => {
                Action::Bid { player, tricks }
            }
            EnvironmentAction::Player(OraclePlayerAction::Play { player, card }) => {
                Action::Play { player, card }
            }
            EnvironmentAction::Chance(chance) => Action::Deal(chance.deck),
            EnvironmentAction::Settle => Action::SettleRound,
        };
        let transition = state.transition(oracle_action)?;
        let round_scores = transition.round_scores.map(|scores| {
            array::from_fn(|index| {
                let score = scores[index];
                RoundScoreEvent {
                    agent: Seat::new(index).expect("array index is a valid seat"),
                    score,
                    score_origin: RuleOrigin {
                        rule_id: match score.outcome {
                            BidOutcome::Missed => "R-SCORE-002",
                            BidOutcome::Exact => "R-SCORE-003",
                            BidOutcome::AllTricks => "R-SCORE-004",
                        },
                    },
                    money_origin: RuleOrigin {
                        rule_id: if score.payment_cents == 0 {
                            "R-MONEY-002"
                        } else {
                            "R-MONEY-001"
                        },
                    },
                }
            })
        });
        let game_outcome = outcome_from_state(transition.next.state());
        Ok(TransitionOutcome {
            state: transition.next,
            round_scores,
            game_outcome,
        })
    }

    fn terminal_status(state: &Self::State) -> TerminalStatus<Self::GameOutcome> {
        outcome_from_state(state.state()).map_or(TerminalStatus::Ongoing, TerminalStatus::Finished)
    }
}

fn outcome_from_state<const PLAYERS: usize>(
    state: &GameState<PLAYERS>,
) -> Option<OracleGameOutcome<PLAYERS>> {
    let GameState::Finished(finished) = state else {
        return None;
    };
    outcome_from_finished(finished)
}

fn outcome_from_finished<const PLAYERS: usize>(
    finished: &Finished<PLAYERS>,
) -> Option<OracleGameOutcome<PLAYERS>> {
    Some(OracleGameOutcome {
        scores: finished.scores,
        pot_cents: finished.pot_cents,
        winners: finished.winners,
        pot_division: finished.pot_division()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type Env = OracleEnvironment<2>;

    fn prepared() -> Game<2> {
        Env::initial(Seat::new(1).unwrap()).unwrap()
    }

    #[test]
    fn contract_separates_action_owners_and_replays_chance() {
        let game = prepared();
        assert_eq!(Env::turn(&game), TurnOwner::Chance);
        assert!(Env::legal_actions(&game).is_empty());

        let first = OracleChanceAction::seeded(0x5eed, 0);
        let replay = OracleChanceAction::seeded(0x5eed, 0);
        let later = OracleChanceAction::seeded(0x5eed, 1);
        assert_eq!(first, replay);
        assert_ne!(first.deck, later.deck);

        let dealt = Env::transition(&game, EnvironmentAction::Chance(first))
            .unwrap()
            .state;
        assert!(matches!(Env::turn(&dealt), TurnOwner::Agent(_)));
        assert!(!Env::legal_actions(&dealt).is_empty());
    }

    #[test]
    fn observation_visibility_exposes_only_viewer_hand() {
        let dealt = Env::transition(
            &prepared(),
            EnvironmentAction::Chance(OracleChanceAction::seeded(7, 0)),
        )
        .unwrap()
        .state;
        let seat0 = Seat::new(0).unwrap();
        let seat1 = Seat::new(1).unwrap();
        let view0 = Env::observe(&dealt, seat0);
        let view1 = Env::observe(&dealt, seat1);
        assert_eq!(view0.viewer, seat0);
        assert_eq!(view1.viewer, seat1);
        assert_eq!(view0.hand_counts, view1.hand_counts);
        assert_ne!(view0.private_hand, view1.private_hand);

        let GameState::Bidding(hidden) = dealt.state() else {
            panic!("deal enters bidding")
        };
        assert_eq!(view0.private_hand, hidden.hands[0]);
        assert_eq!(view1.private_hand, hidden.hands[1]);
        assert!(
            !view0
                .private_hand
                .iter()
                .any(|card| hidden.hands[1].contains(card))
        );
    }

    #[test]
    fn round_score_events_are_raw_and_rule_originated() {
        let mut game = Env::transition(
            &prepared(),
            EnvironmentAction::Chance(OracleChanceAction::seeded(42, 0)),
        )
        .unwrap()
        .state;
        let mut event = None;
        for _ in 0..32 {
            let action = match Env::turn(&game) {
                TurnOwner::Agent(_) => EnvironmentAction::Player(
                    Env::legal_actions(&game)
                        .into_iter()
                        .next()
                        .expect("acting agent has a legal action"),
                ),
                TurnOwner::Environment => EnvironmentAction::Settle,
                TurnOwner::Chance | TurnOwner::Finished => break,
            };
            let outcome = Env::transition(&game, action).unwrap();
            if outcome.round_scores.is_some() {
                event = outcome.round_scores;
            }
            game = outcome.state;
            if event.is_some() {
                break;
            }
        }
        let scores = event.expect("one-card round reaches its score boundary");
        for score in scores {
            assert!(score.score.points <= 21);
            assert!(score.score_origin.rule_id.starts_with("R-SCORE-"));
            assert!(score.money_origin.rule_id.starts_with("R-MONEY-"));
        }
        assert!(matches!(
            Env::terminal_status(&game),
            TerminalStatus::Ongoing
        ));
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Executable comparisons between independently authored Poche models.
//!
//! The strict Rust model has two players and six cards; the conventional
//! oracle has the complete 52-card deck. This crate embeds the strict cards as
//! the lowest three ranks of two oracle suits. It compares the common `1, 2`
//! round prefix exactly, while preserving (rather than hiding) scope changes.

mod alloy;
mod consensus_compare;
mod consensus_coverage;
mod governance_compare;
mod nusmv;
mod prolog;
mod spatial_alloy;
mod spatial_compare;
mod spatial_coverage;
mod spatial_nusmv;
mod spatial_prolog;

pub use alloy::{AlloyConformanceError, AlloyConformanceReport, compare_rust_alloy};
pub use consensus_compare::{
    ConsensusComparisonError, ConsensusComparisonReport, compare_consensus_models,
};
pub use consensus_coverage::{
    ConsensusCoverageError, ConsensusCoverageReport, audit_consensus_coverage,
};
pub use governance_compare::{
    GovernanceComparisonError, GovernanceComparisonReport, compare_governance_models,
};
pub use nusmv::{NuSmvConformanceError, NuSmvConformanceReport, compare_rust_nusmv};

pub use prolog::{
    PrologConformanceError, PrologConformanceReport, PrologFixtureComparison, PrologFixtureSpec,
    compare_rust_prolog,
};
pub use spatial_alloy::{SpatialAlloyError, SpatialAlloyReport, check_spatial_alloy_layout_micro};
pub use spatial_compare::{
    SpatialComparisonError, SpatialComparisonReport, compare_spatial_models,
};
pub use spatial_coverage::{SpatialCoverageError, SpatialCoverageReport, audit_spatial_coverage};
pub use spatial_nusmv::{
    SpatialNuSmvError, SpatialNuSmvReport, check_spatial_nusmv_transition_micro,
};
pub use spatial_prolog::{
    SpatialPrologError, SpatialPrologFixture, SpatialPrologReport, check_spatial_prolog_query_micro,
};

use std::error::Error;
use std::fmt;

use poche_model::{
    Bid as FormalBid, Card as FormalCard, CardSet, ChanceAction, Deal, Game as FormalGame,
    LegalActions, ModelAction, OneCardDeal, Phase as FormalPhase, Player as FormalPlayer,
    PlayerAction, RoundId, RoundOutcome, TwoCardDeal,
};
use poche_oracle_rust::{
    Action as OracleAction, BidOutcome, Card as OracleCard, DECK_SIZE, DeckOrder,
    Game as OracleGame, GameState as OracleState, PhaseTag as OraclePhase, Rank, Seat, Suit,
    Turn as OracleTurn,
};

/// Nature of one comparison fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureKind {
    /// A scenario from `fixtures/oracle-inventory.toml`.
    Scenario,
    /// A cross-model property from the same inventory.
    Property,
    /// A shared query surface from the same inventory.
    Query,
}

/// A known reason why literal values cannot be equal between both models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DifferenceClass {
    /// The strict named scope uses schedule `1,2,1`; the full oracle uses
    /// `1..7..1` for two players.
    ScheduleScope,
    /// Six abstract cards preserve suit/rank decisions but are not 52 cards.
    CardUniverseScope,
    /// The strict model starts after physical first-dealer selection.
    PreparedDealerBoundary,
}

/// Audited outcome of one fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Executable projections agreed.
    Match,
    /// A literal difference is expected and classified.
    ClassifiedDifference(DifferenceClass),
}

/// One durable result tied to the authoritative rule IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseResult {
    /// Stable fixture ID.
    pub id: &'static str,
    /// Inventory category.
    pub kind: FixtureKind,
    /// Audited result.
    pub disposition: Disposition,
    /// Rules explaining the comparison or difference.
    pub rule_ids: &'static [&'static str],
    /// Human-readable evidence summary.
    pub evidence: &'static str,
}

/// Complete conventional-versus-strict Rust comparison report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonReport {
    /// Every scenario plus the cross-Rust property/query fixtures.
    pub cases: Vec<CaseResult>,
    /// Exact normalized observations compared in the common prefix.
    pub observations_compared: usize,
    /// Exact legal player-action sets compared in the common prefix.
    pub legal_action_sets_compared: usize,
    /// Exact paired semantic transitions applied.
    pub transitions_compared: usize,
}

impl ComparisonReport {
    /// Number of literal matches.
    #[must_use]
    pub fn match_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.disposition == Disposition::Match)
            .count()
    }

    /// Number of explicit, classified differences.
    #[must_use]
    pub fn difference_count(&self) -> usize {
        self.cases.len() - self.match_count()
    }
}

/// A mismatch in an area declared common to both models.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceError {
    context: &'static str,
    detail: String,
}

impl fmt::Display for ConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.context, self.detail)
    }
}

impl Error for ConformanceError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NormalPhase {
    AwaitingDeal,
    Bidding,
    Playing,
    Scoring,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalObservation {
    phase: NormalPhase,
    viewer: u8,
    dealer: Option<u8>,
    actor: NormalActor,
    round_index: Option<u8>,
    hand_size: u8,
    private_hand: Vec<u8>,
    hand_counts: [u8; 2],
    trump: Option<u8>,
    current_trick: Vec<(u8, u8)>,
    bids: [Option<u8>; 2],
    tricks_won: [u8; 2],
    scores: [u16; 2],
    pot_cents: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NormalActor {
    Chance,
    Player(u8),
    Environment,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NormalScoreOutcome {
    Miss,
    Exact,
    AllTricks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NormalScore {
    bid: u8,
    tricks: u8,
    outcome: NormalScoreOutcome,
    points: u16,
    payment_cents: u32,
}

struct PairedGame {
    formal: FormalGame,
    oracle: OracleGame<2>,
    observations: usize,
    legal_sets: usize,
    transitions: usize,
}

impl PairedGame {
    fn new(dealer: FormalPlayer) -> Result<Self, ConformanceError> {
        let oracle = OracleGame::new(oracle_seat(dealer))
            .map_err(|error| mismatch("construct conventional oracle", format!("{error:?}")))?;
        let mut paired = Self {
            formal: FormalGame::new(dealer),
            oracle,
            observations: 0,
            legal_sets: 0,
            transitions: 0,
        };
        paired.compare_observations("prepared initial state")?;
        Ok(paired)
    }

    fn deal(&mut self, deal: Deal) -> Result<(), ConformanceError> {
        let formal = self
            .formal
            .transition(ModelAction::Chance(ChanceAction::Deal(deal)))
            .map_err(|error| mismatch("strict deal", error.to_string()))?;
        let deck = oracle_deck(deal, current_formal_dealer(self.formal)?)?;
        let oracle = self
            .oracle
            .transition(OracleAction::Deal(deck))
            .map_err(|error| mismatch("conventional deal", format!("{error:?}")))?;
        self.formal = formal.next;
        self.oracle = oracle.next;
        self.transitions += 1;
        self.compare_observations("after deal")
    }

    fn bid(&mut self, bid: FormalBid) -> Result<(), ConformanceError> {
        self.compare_legal_actions("before bid")?;
        let formal_player = current_formal_player(self.formal)?;
        let oracle_player = current_oracle_player(&self.oracle)?;
        ensure_equal(
            "bid actor",
            finite_index(formal_player.index()),
            u8::try_from(oracle_player.index()).unwrap_or(u8::MAX),
        )?;
        let formal = self
            .formal
            .transition(ModelAction::Player(PlayerAction::Bid {
                player: formal_player,
                bid,
            }))
            .map_err(|error| mismatch("strict bid", error.to_string()))?;
        let oracle = self
            .oracle
            .transition(OracleAction::Bid {
                player: oracle_player,
                tricks: bid.get(),
            })
            .map_err(|error| mismatch("conventional bid", format!("{error:?}")))?;
        self.formal = formal.next;
        self.oracle = oracle.next;
        self.transitions += 1;
        self.compare_observations("after bid")
    }

    fn play(&mut self, card: FormalCard) -> Result<(), ConformanceError> {
        self.compare_legal_actions("before play")?;
        let formal_player = current_formal_player(self.formal)?;
        let oracle_player = current_oracle_player(&self.oracle)?;
        ensure_equal(
            "play actor",
            finite_index(formal_player.index()),
            u8::try_from(oracle_player.index()).unwrap_or(u8::MAX),
        )?;
        let formal = self
            .formal
            .transition(ModelAction::Player(PlayerAction::Play {
                player: formal_player,
                card,
            }))
            .map_err(|error| mismatch("strict play", error.to_string()))?;
        let oracle = self
            .oracle
            .transition(OracleAction::Play {
                player: oracle_player,
                card: to_oracle_card(card),
            })
            .map_err(|error| mismatch("conventional play", format!("{error:?}")))?;
        self.formal = formal.next;
        self.oracle = oracle.next;
        self.transitions += 1;
        self.compare_observations("after play")
    }

    fn settle(&mut self, compare_successor: bool) -> Result<[NormalScore; 2], ConformanceError> {
        let formal = self
            .formal
            .transition(ModelAction::Settle)
            .map_err(|error| mismatch("strict settlement", error.to_string()))?;
        let oracle = self
            .oracle
            .transition(OracleAction::SettleRound)
            .map_err(|error| mismatch("conventional settlement", format!("{error:?}")))?;
        let formal_scores = formal
            .round_scores
            .ok_or_else(|| mismatch("strict settlement", "missing round scores"))?
            .map(|event| NormalScore {
                bid: event.bid.get(),
                tricks: event.tricks_won,
                outcome: match event.outcome {
                    RoundOutcome::Miss => NormalScoreOutcome::Miss,
                    RoundOutcome::Exact => NormalScoreOutcome::Exact,
                    RoundOutcome::AllTricks => NormalScoreOutcome::AllTricks,
                },
                points: u16::from(event.points),
                payment_cents: u32::from(event.payment_cents),
            });
        let oracle_scores = oracle
            .round_scores
            .ok_or_else(|| mismatch("conventional settlement", "missing round scores"))?
            .map(|event| NormalScore {
                bid: event.bid,
                tricks: event.tricks,
                outcome: match event.outcome {
                    BidOutcome::Missed => NormalScoreOutcome::Miss,
                    BidOutcome::Exact => NormalScoreOutcome::Exact,
                    BidOutcome::AllTricks => NormalScoreOutcome::AllTricks,
                },
                points: event.points,
                payment_cents: event.payment_cents,
            });
        ensure_equal("round score events", formal_scores, oracle_scores)?;
        self.formal = formal.next;
        self.oracle = oracle.next;
        self.transitions += 1;
        if compare_successor {
            self.compare_observations("after settlement")?;
        }
        Ok(formal_scores)
    }

    fn compare_observations(&mut self, context: &'static str) -> Result<(), ConformanceError> {
        for player in FormalPlayer::ALL {
            let formal = normalize_formal_observation(self.formal.observe(player));
            let oracle = normalize_oracle_observation(&self.oracle, oracle_seat(player))?;
            ensure_equal(context, formal, oracle)?;
            self.observations += 1;
        }
        Ok(())
    }

    fn compare_legal_actions(&mut self, context: &'static str) -> Result<(), ConformanceError> {
        let formal = normalize_formal_actions(self.formal.legal_actions())?;
        let oracle = normalize_oracle_actions(self.oracle.legal_player_actions())?;
        ensure_equal(context, formal, oracle)?;
        self.legal_sets += 1;
        Ok(())
    }
}

/// Execute the complete conventional-versus-strict Rust conformance corpus.
///
/// # Errors
///
/// Returns the first unclassified mismatch in a projection declared common.
#[allow(
    clippy::too_many_lines,
    reason = "the literal report is intentionally kept beside its executable evidence gate"
)]
pub fn compare_rust_models() -> Result<ComparisonReport, ConformanceError> {
    let prefix = compare_common_prefix()?;
    compare_unrestricted_bid_total()?;
    compare_void_trump_and_off_suit()?;
    let terminal = compare_terminal_semantics()?;

    ensure_equal("strict final hand size", terminal.strict_final_hand, 1_u8)?;
    ensure_equal("oracle final hand size", terminal.oracle_final_hand, 1_u8)?;

    let cases = vec![
        difference(
            "schedule-boundaries",
            FixtureKind::Scenario,
            DifferenceClass::ScheduleScope,
            &[
                "R-GAME-001",
                "R-HAND-001",
                "R-HAND-002",
                "R-HAND-003",
                "R-HAND-004",
            ],
            "common prefix is 1,2; strict continues 1 and full two-player oracle continues 3..7..1",
        ),
        matched(
            "complete-two-player-round",
            FixtureKind::Scenario,
            &["R-GAME-003", "R-DEAL-001", "R-TRICK-001", "R-SCORE-001"],
            "two complete rounds agree step-by-step",
        ),
        matched(
            "unrestricted-total-bid",
            FixtureKind::Scenario,
            &["R-BID-004"],
            "both models admit total bid 2 in a one-trick round",
        ),
        matched(
            "zero-bid-success",
            FixtureKind::Scenario,
            &["R-BID-005", "R-SCORE-003"],
            "paired round produces the same exact zero-bid score",
        ),
        matched(
            "follow-suit-required",
            FixtureKind::Scenario,
            &["R-TRICK-005"],
            "normalized legal card set removes the same off-suit card",
        ),
        matched(
            "void-player-may-trump",
            FixtureKind::Scenario,
            &["R-TRICK-006", "R-TRICK-007"],
            "paired void follower may play trump and wins",
        ),
        matched(
            "off-suit-cannot-win",
            FixtureKind::Scenario,
            &["R-TRICK-008", "R-TRICK-009", "R-TRICK-011"],
            "paired non-trump off-suit follower loses",
        ),
        matched(
            "exact-partial-score",
            FixtureKind::Scenario,
            &["R-SCORE-003", "R-SCORE-005"],
            "round score events and cumulative points agree",
        ),
        matched(
            "all-tricks-score",
            FixtureKind::Scenario,
            &["R-SCORE-004", "R-SCORE-005"],
            "round score events agree on the all-tricks bonus",
        ),
        matched(
            "missed-bid-payment",
            FixtureKind::Scenario,
            &["R-SCORE-002", "R-MONEY-001", "R-MONEY-002"],
            "miss score and ten-cent pot increment agree",
        ),
        matched(
            "shared-final-winner",
            FixtureKind::Scenario,
            &["R-GAME-005", "R-MONEY-003", "R-FINISH-003", "R-FINISH-004"],
            "both completed models select exactly every maximum-score seat and divide the separate pot",
        ),
        difference(
            "first-jack-seat-order",
            FixtureKind::Scenario,
            DifferenceClass::PreparedDealerBoundary,
            &["R-RANDOM-002", "R-RANDOM-003", "R-RANDOM-006"],
            "strict model accepts the selected first dealer as prepared input",
        ),
        difference(
            "high-card-repeated-tie",
            FixtureKind::Scenario,
            DifferenceClass::PreparedDealerBoundary,
            &["R-RANDOM-004", "R-RANDOM-005", "R-RANDOM-006"],
            "strict model accepts the selected first dealer as prepared input",
        ),
        matched(
            "dealer-rotation",
            FixtureKind::Scenario,
            &["R-GAME-004", "R-ADVANCE-002"],
            "dealer and acting seat agree across both common rounds",
        ),
        matched(
            "final-one-card-round",
            FixtureKind::Scenario,
            &["R-ADVANCE-003", "R-FINISH-001"],
            "both complete schedules end with a normally scored one-card round",
        ),
        difference(
            "full-deck-conservation",
            FixtureKind::Property,
            DifferenceClass::CardUniverseScope,
            &["R-GAME-002", "R-DEAL-004", "R-TRICK-012", "R-ADVANCE-001"],
            "each model conserves its declared card universe; cardinalities intentionally differ",
        ),
        matched(
            "private-observation-does-not-leak",
            FixtureKind::Property,
            &["R-GAME-004", "R-BID-003", "R-TRICK-004"],
            "both viewer projections agree throughout the exact common prefix",
        ),
        matched(
            "legal-actions-from-state",
            FixtureKind::Query,
            &["R-BID-002", "R-TRICK-003", "R-TRICK-005", "R-TRICK-006"],
            "normalized legal bid/play sets agree before every common-prefix player action",
        ),
    ];

    Ok(ComparisonReport {
        cases,
        observations_compared: prefix.observations,
        legal_action_sets_compared: prefix.legal_sets,
        transitions_compared: prefix.transitions,
    })
}

fn compare_common_prefix() -> Result<PairedGame, ConformanceError> {
    let mut paired = PairedGame::new(FormalPlayer::One)?;
    paired.deal(first_deal())?;
    assert_private_projection(&paired)?;
    paired.bid(FormalBid::Zero)?;
    paired.bid(FormalBid::One)?;
    paired.play(FormalCard::S0R0)?;
    paired.play(FormalCard::S0R1)?;
    let first_scores = paired.settle(true)?;
    ensure_equal("zero-bid exact score", first_scores[0].points, 10)?;
    ensure_equal(
        "one-bid all-tricks outcome",
        first_scores[1].outcome,
        NormalScoreOutcome::AllTricks,
    )?;

    paired.deal(second_deal())?;
    paired.bid(FormalBid::One)?;
    paired.bid(FormalBid::Two)?;
    paired.play(FormalCard::S0R1)?;
    paired.play(FormalCard::S0R0)?;
    paired.play(FormalCard::S1R0)?;
    paired.play(FormalCard::S1R2)?;
    let second_scores = paired.settle(false)?;
    ensure_equal(
        "missed-bid outcome",
        second_scores[0].outcome,
        NormalScoreOutcome::Miss,
    )?;
    ensure_equal("missed-bid payment", second_scores[0].payment_cents, 10)?;
    ensure_equal(
        "partial exact outcome",
        second_scores[1].outcome,
        NormalScoreOutcome::Exact,
    )?;

    let formal = normalize_formal_observation(paired.formal.observe(FormalPlayer::Zero));
    let oracle = normalize_oracle_observation(&paired.oracle, oracle_seat(FormalPlayer::Zero))?;
    ensure_equal("post-prefix scores", formal.scores, oracle.scores)?;
    ensure_equal("post-prefix pot", formal.pot_cents, oracle.pot_cents)?;
    ensure_equal("post-prefix dealer", formal.dealer, oracle.dealer)?;
    ensure_equal("post-prefix phase", formal.phase, oracle.phase)?;
    ensure_equal("strict split hand", formal.hand_size, 1)?;
    ensure_equal("oracle split hand", oracle.hand_size, 3)?;
    Ok(paired)
}

fn compare_unrestricted_bid_total() -> Result<(), ConformanceError> {
    let mut paired = PairedGame::new(FormalPlayer::One)?;
    paired.deal(first_deal())?;
    paired.bid(FormalBid::One)?;
    paired.bid(FormalBid::One)?;
    ensure_equal(
        "unrestricted bid phase",
        paired.formal.phase(),
        FormalPhase::Playing,
    )
}

fn compare_void_trump_and_off_suit() -> Result<(), ConformanceError> {
    let void_deal = one_card_deal(FormalCard::S0R0, FormalCard::S1R0, FormalCard::S1R2)?;
    let void_winner = compare_one_trick(void_deal, FormalCard::S0R0, FormalCard::S1R0)?;
    ensure_equal("void trump winner", void_winner, [0, 1])?;

    let off_suit_deal = one_card_deal(FormalCard::S0R2, FormalCard::S1R2, FormalCard::S0R0)?;
    let off_suit_winner = compare_one_trick(off_suit_deal, FormalCard::S0R2, FormalCard::S1R2)?;
    ensure_equal("off-suit loser", off_suit_winner, [1, 0])
}

fn compare_one_trick(
    deal: Deal,
    lead: FormalCard,
    follow: FormalCard,
) -> Result<[u8; 2], ConformanceError> {
    let mut paired = PairedGame::new(FormalPlayer::One)?;
    paired.deal(deal)?;
    paired.bid(FormalBid::Zero)?;
    paired.bid(FormalBid::Zero)?;
    paired.play(lead)?;
    paired.play(follow)?;
    let formal = normalize_formal_observation(paired.formal.observe(FormalPlayer::Zero));
    let oracle = normalize_oracle_observation(&paired.oracle, oracle_seat(FormalPlayer::Zero))?;
    ensure_equal("one-trick counts", formal.tricks_won, oracle.tricks_won)?;
    Ok(formal.tricks_won)
}

struct TerminalEvidence {
    strict_final_hand: u8,
    oracle_final_hand: u8,
}

fn compare_terminal_semantics() -> Result<TerminalEvidence, ConformanceError> {
    let (formal, strict_final_hand) = finish_formal()?;
    let (oracle, oracle_final_hand) = finish_oracle()?;
    let poche_model::Game::Finished(formal) = formal else {
        return Err(mismatch("strict terminal", "did not finish"));
    };
    let OracleState::Finished(oracle) = oracle.state() else {
        return Err(mismatch("conventional terminal", "did not finish"));
    };
    let formal_scores = formal.scores().map(|score| u16::from(score.get()));
    ensure_equal(
        "strict winner mask",
        formal.winners(),
        maximum_mask(formal_scores),
    )?;
    ensure_equal(
        "oracle winner mask",
        oracle.winners,
        maximum_mask(oracle.scores),
    )?;
    let formal_outcome = FormalGame::Finished(formal)
        .terminal_outcome()
        .ok_or_else(|| mismatch("strict terminal", "missing outcome"))?;
    let formal_division = formal_outcome.pot_division;
    let oracle_division = oracle
        .pot_division()
        .ok_or_else(|| mismatch("conventional terminal", "missing pot division"))?;
    ensure_equal(
        "strict pot division arithmetic",
        u16::from(formal_division.winner_count) * formal_division.cents_per_winner
            + formal_division.remainder_cents,
        formal.pot().cents(),
    )?;
    ensure_equal(
        "oracle pot division arithmetic",
        u32::try_from(oracle_division.winner_count).unwrap_or(u32::MAX)
            * oracle_division.cents_per_winner
            + oracle_division.remainder_cents,
        oracle.pot_cents,
    )?;
    Ok(TerminalEvidence {
        strict_final_hand,
        oracle_final_hand,
    })
}

fn finish_formal() -> Result<(FormalGame, u8), ConformanceError> {
    let mut game = FormalGame::new(FormalPlayer::One);
    let mut final_hand = 0;
    for _ in 0..100 {
        match game.legal_actions() {
            LegalActions::Chance(actions) => {
                let action = actions
                    .into_iter()
                    .next()
                    .ok_or_else(|| mismatch("strict terminal", "empty chance set"))?;
                let ChanceAction::Deal(deal) = action;
                final_hand = deal.hand_size();
                game = game
                    .transition(ModelAction::Chance(action))
                    .map_err(|error| mismatch("strict terminal chance", error.to_string()))?
                    .next;
            }
            LegalActions::Player(actions) => {
                let action = actions
                    .into_iter()
                    .next()
                    .ok_or_else(|| mismatch("strict terminal", "empty player set"))?;
                game = game
                    .transition(ModelAction::Player(action))
                    .map_err(|error| mismatch("strict terminal player", error.to_string()))?
                    .next;
            }
            LegalActions::Environment => {
                game = game
                    .transition(ModelAction::Settle)
                    .map_err(|error| mismatch("strict terminal settle", error.to_string()))?
                    .next;
            }
            LegalActions::Finished => return Ok((game, final_hand)),
        }
    }
    Err(mismatch("strict terminal", "step bound exceeded"))
}

fn finish_oracle() -> Result<(OracleGame<2>, u8), ConformanceError> {
    let mut game = OracleGame::new(oracle_seat(FormalPlayer::One))
        .map_err(|error| mismatch("conventional terminal", format!("{error:?}")))?;
    let mut final_hand = 0;
    for _ in 0..1000 {
        match game.turn() {
            OracleTurn::Chance => {
                game = game
                    .transition(OracleAction::Deal(DeckOrder::standard()))
                    .map_err(|error| mismatch("conventional terminal deal", format!("{error:?}")))?
                    .next;
                final_hand = game.observe(oracle_seat(FormalPlayer::Zero)).hand_size;
            }
            OracleTurn::Player(_) => {
                let action = game
                    .legal_player_actions()
                    .into_iter()
                    .next()
                    .ok_or_else(|| mismatch("conventional terminal", "empty player set"))?;
                game = game
                    .transition(action)
                    .map_err(|error| {
                        mismatch("conventional terminal player", format!("{error:?}"))
                    })?
                    .next;
            }
            OracleTurn::Environment => {
                game = game
                    .transition(OracleAction::SettleRound)
                    .map_err(|error| {
                        mismatch("conventional terminal settle", format!("{error:?}"))
                    })?
                    .next;
            }
            OracleTurn::Finished => return Ok((game, final_hand)),
        }
    }
    Err(mismatch("conventional terminal", "step bound exceeded"))
}

fn first_deal() -> Deal {
    one_card_deal(FormalCard::S0R0, FormalCard::S0R1, FormalCard::S1R2)
        .expect("fixed first deal partitions six unique cards")
}

fn second_deal() -> Deal {
    Deal::Two(
        TwoCardDeal::new(
            [
                CardSet::from_cards([FormalCard::S0R0, FormalCard::S1R2]).unwrap(),
                CardSet::from_cards([FormalCard::S0R1, FormalCard::S1R0]).unwrap(),
            ],
            FormalCard::S0R2,
            CardSet::from_cards([FormalCard::S1R1]).unwrap(),
        )
        .expect("fixed second deal partitions six unique cards"),
    )
}

fn one_card_deal(
    zero: FormalCard,
    one: FormalCard,
    trump: FormalCard,
) -> Result<Deal, ConformanceError> {
    let undealt = CardSet::from_cards(
        FormalCard::ALL
            .into_iter()
            .filter(|card| ![zero, one, trump].contains(card)),
    )
    .map_err(|error| mismatch("one-card fixture", error.to_string()))?;
    OneCardDeal::new([zero, one], trump, undealt)
        .map(Deal::One)
        .map_err(|error| mismatch("one-card fixture", error.to_string()))
}

#[allow(
    clippy::needless_range_loop,
    reason = "layer indexes both player hands while deal order alternates by dealer"
)]
fn oracle_deck(deal: Deal, dealer: FormalPlayer) -> Result<DeckOrder, ConformanceError> {
    let hands = deal.hands().map(|hand| hand.iter().collect::<Vec<_>>());
    let first = dealer.left();
    let mut prefix = Vec::with_capacity(usize::from(deal.hand_size()) * 2 + 1);
    for layer in 0..usize::from(deal.hand_size()) {
        for offset in 0..2 {
            let player = if offset == 0 { first } else { first.left() };
            prefix.push(to_oracle_card(hands[player.index()][layer]));
        }
    }
    prefix.push(to_oracle_card(deal.trump()));
    let mut cards = prefix;
    let remainder = OracleCard::standard_deck()
        .into_iter()
        .filter(|card| !cards.contains(card))
        .collect::<Vec<_>>();
    cards.extend(remainder);
    let cards: [OracleCard; DECK_SIZE] = cards.try_into().map_err(|cards: Vec<_>| {
        mismatch("embedded oracle deck", format!("has {} cards", cards.len()))
    })?;
    DeckOrder::new(cards).map_err(|error| mismatch("embedded oracle deck", format!("{error:?}")))
}

fn to_oracle_card(card: FormalCard) -> OracleCard {
    let suit = if card.suit() == 0 {
        Suit::Clubs
    } else {
        Suit::Diamonds
    };
    let rank = match card.rank() {
        0 => Rank::Two,
        1 => Rank::Three,
        _ => Rank::Four,
    };
    OracleCard::new(suit, rank)
}

fn from_oracle_card(card: OracleCard) -> Result<FormalCard, ConformanceError> {
    FormalCard::ALL
        .into_iter()
        .find(|candidate| to_oracle_card(*candidate) == card)
        .ok_or_else(|| mismatch("card embedding", format!("unmapped oracle card {card:?}")))
}

fn normalize_formal_observation(observation: poche_model::Observation) -> NormalObservation {
    NormalObservation {
        phase: match observation.phase {
            FormalPhase::AwaitingDeal => NormalPhase::AwaitingDeal,
            FormalPhase::Bidding => NormalPhase::Bidding,
            FormalPhase::Playing => NormalPhase::Playing,
            FormalPhase::Scoring => NormalPhase::Scoring,
            FormalPhase::Finished => NormalPhase::Finished,
        },
        viewer: finite_index(observation.viewer.index()),
        dealer: observation
            .dealer
            .map(|player| finite_index(player.index())),
        actor: match observation.actor {
            poche_model::TurnOwner::Chance => NormalActor::Chance,
            poche_model::TurnOwner::Player(player) => {
                NormalActor::Player(finite_index(player.index()))
            }
            poche_model::TurnOwner::Environment => NormalActor::Environment,
            poche_model::TurnOwner::Finished => NormalActor::Finished,
        },
        round_index: observation.round.map(formal_round_index),
        hand_size: observation.round.map_or(0, RoundId::hand_size),
        private_hand: observation
            .private_hand
            .iter()
            .map(|card| finite_index(card.index()))
            .collect(),
        hand_counts: observation.hand_counts,
        trump: observation.trump.map(|card| finite_index(card.index())),
        current_trick: observation
            .current_trick
            .into_iter()
            .map(|play| {
                (
                    finite_index(play.player.index()),
                    finite_index(play.card.index()),
                )
            })
            .collect(),
        bids: observation.bids.map(|bid| bid.map(FormalBid::get)),
        tricks_won: observation.tricks_won.map(poche_model::Tricks::get),
        scores: observation.scores.map(|score| u16::from(score.get())),
        pot_cents: u32::from(observation.pot.cents()),
    }
}

fn normalize_oracle_observation(
    game: &OracleGame<2>,
    viewer: Seat<2>,
) -> Result<NormalObservation, ConformanceError> {
    let observation = game.observe(viewer);
    let private_hand = observation
        .private_hand
        .iter()
        .map(from_oracle_card)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|card| finite_index(card.index()))
        .collect();
    let current_trick = observation
        .current_trick
        .iter()
        .map(|play| {
            Ok((
                u8::try_from(play.player.index()).unwrap_or(u8::MAX),
                finite_index(from_oracle_card(play.card)?.index()),
            ))
        })
        .collect::<Result<Vec<_>, ConformanceError>>()?;
    Ok(NormalObservation {
        phase: match observation.phase {
            OraclePhase::AwaitingDeal => NormalPhase::AwaitingDeal,
            OraclePhase::Bidding => NormalPhase::Bidding,
            OraclePhase::Playing => NormalPhase::Playing,
            OraclePhase::Scoring => NormalPhase::Scoring,
            OraclePhase::Finished => NormalPhase::Finished,
        },
        viewer: u8::try_from(viewer.index()).unwrap_or(u8::MAX),
        dealer: observation
            .dealer
            .map(|seat| u8::try_from(seat.index()).unwrap_or(u8::MAX)),
        actor: match observation.actor {
            OracleTurn::Chance => NormalActor::Chance,
            OracleTurn::Player(seat) => {
                NormalActor::Player(u8::try_from(seat.index()).unwrap_or(u8::MAX))
            }
            OracleTurn::Environment => NormalActor::Environment,
            OracleTurn::Finished => NormalActor::Finished,
        },
        round_index: (observation.phase != OraclePhase::Finished)
            .then(|| u8::try_from(observation.round_index).unwrap_or(u8::MAX)),
        hand_size: observation.hand_size,
        private_hand,
        hand_counts: observation.hand_counts,
        trump: observation
            .trump
            .map(from_oracle_card)
            .transpose()?
            .map(|card| finite_index(card.index())),
        current_trick,
        bids: observation.bids,
        tricks_won: observation.tricks_won,
        scores: observation.scores,
        pot_cents: observation.pot_cents,
    })
}

fn normalize_formal_actions(actions: LegalActions) -> Result<Vec<String>, ConformanceError> {
    let LegalActions::Player(actions) = actions else {
        return Err(mismatch(
            "strict legal actions",
            "expected player-owned set",
        ));
    };
    let mut actions = actions
        .into_iter()
        .map(|action| match action {
            PlayerAction::Bid { player, bid } => {
                format!("bid:{}:{}", player.index(), bid.get())
            }
            PlayerAction::Play { player, card } => {
                format!("play:{}:{}", player.index(), card.index())
            }
        })
        .collect::<Vec<_>>();
    actions.sort();
    Ok(actions)
}

fn normalize_oracle_actions(
    actions: Vec<OracleAction<2>>,
) -> Result<Vec<String>, ConformanceError> {
    let mut result = Vec::with_capacity(actions.len());
    for action in actions {
        result.push(match action {
            OracleAction::Bid { player, tricks } => {
                format!("bid:{}:{tricks}", player.index())
            }
            OracleAction::Play { player, card } => {
                format!(
                    "play:{}:{}",
                    player.index(),
                    from_oracle_card(card)?.index()
                )
            }
            OracleAction::Deal(_) | OracleAction::SettleRound => {
                return Err(mismatch(
                    "conventional legal actions",
                    "player action set contained chance/environment action",
                ));
            }
        });
    }
    result.sort();
    Ok(result)
}

fn assert_private_projection(paired: &PairedGame) -> Result<(), ConformanceError> {
    let FormalGame::Bidding(formal) = paired.formal else {
        return Err(mismatch("private projection", "strict model not bidding"));
    };
    let OracleState::Bidding(oracle) = paired.oracle.state() else {
        return Err(mismatch("private projection", "oracle not bidding"));
    };
    for player in FormalPlayer::ALL {
        let formal_observation = paired.formal.observe(player);
        ensure_equal(
            "strict viewer hand",
            formal_observation.private_hand,
            formal.hand(player),
        )?;
        let oracle_observation = paired.oracle.observe(oracle_seat(player));
        ensure_equal(
            "oracle viewer hand",
            oracle_observation.private_hand.clone(),
            oracle.hands[player.index()].clone(),
        )?;
    }
    Ok(())
}

fn current_formal_dealer(game: FormalGame) -> Result<FormalPlayer, ConformanceError> {
    match game {
        FormalGame::AwaitingDeal(state) => Ok(state.ledger().dealer()),
        _ => Err(mismatch("strict dealer", "expected awaiting-deal state")),
    }
}

fn current_formal_player(game: FormalGame) -> Result<FormalPlayer, ConformanceError> {
    match game.turn() {
        poche_model::TurnOwner::Player(player) => Ok(player),
        _ => Err(mismatch("strict actor", "expected player turn")),
    }
}

fn current_oracle_player(game: &OracleGame<2>) -> Result<Seat<2>, ConformanceError> {
    match game.turn() {
        OracleTurn::Player(player) => Ok(player),
        _ => Err(mismatch("conventional actor", "expected player turn")),
    }
}

fn oracle_seat(player: FormalPlayer) -> Seat<2> {
    Seat::new(player.index()).expect("formal seat always fits two-player oracle")
}

const fn formal_round_index(round: RoundId) -> u8 {
    match round {
        RoundId::OneAscending => 0,
        RoundId::Two => 1,
        RoundId::OneDescending => 2,
    }
}

fn maximum_mask(scores: [u16; 2]) -> [bool; 2] {
    let maximum = scores[0].max(scores[1]);
    scores.map(|score| score == maximum)
}

fn matched(
    id: &'static str,
    kind: FixtureKind,
    rule_ids: &'static [&'static str],
    evidence: &'static str,
) -> CaseResult {
    CaseResult {
        id,
        kind,
        disposition: Disposition::Match,
        rule_ids,
        evidence,
    }
}

fn difference(
    id: &'static str,
    kind: FixtureKind,
    class: DifferenceClass,
    rule_ids: &'static [&'static str],
    evidence: &'static str,
) -> CaseResult {
    CaseResult {
        id,
        kind,
        disposition: Disposition::ClassifiedDifference(class),
        rule_ids,
        evidence,
    }
}

fn mismatch(context: &'static str, detail: impl Into<String>) -> ConformanceError {
    ConformanceError {
        context,
        detail: detail.into(),
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "owned values make mismatch diagnostics self-contained without borrowing temporaries"
)]
fn ensure_equal<T>(context: &'static str, left: T, right: T) -> Result<(), ConformanceError>
where
    T: fmt::Debug + PartialEq,
{
    if left == right {
        Ok(())
    } else {
        Err(mismatch(
            context,
            format!("strict={left:?}, conventional={right:?}"),
        ))
    }
}

fn finite_index(index: usize) -> u8 {
    u8::try_from(index).expect("formal micro-domain indices fit u8")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn rust_models_compare_the_complete_shared_scenario_inventory() {
        let report = compare_rust_models().expect("all common projections agree");
        assert_eq!(report.cases.len(), 18);
        assert_eq!(report.match_count(), 14);
        assert_eq!(report.difference_count(), 4);
        assert_eq!(report.observations_compared, 28);
        assert_eq!(report.legal_action_sets_compared, 10);
        assert_eq!(report.transitions_compared, 14);
        assert!(report.cases.iter().all(|case| !case.rule_ids.is_empty()));

        let inventory = include_str!("../../../fixtures/oracle-inventory.toml");
        let mut in_scenarios = false;
        let scenario_ids = inventory
            .lines()
            .filter_map(|line| {
                if line == "[[scenario]]" {
                    in_scenarios = true;
                    return None;
                }
                if line == "[[property]]" {
                    in_scenarios = false;
                }
                in_scenarios
                    .then(|| line.strip_prefix("id = \"")?.strip_suffix('"'))
                    .flatten()
            })
            .collect::<BTreeSet<_>>();
        let report_ids = report
            .cases
            .iter()
            .filter(|case| case.kind == FixtureKind::Scenario)
            .map(|case| case.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(report_ids, scenario_ids);
    }
}

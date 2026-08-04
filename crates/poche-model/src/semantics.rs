use std::fmt;

use facet::Facet;

use crate::formal;
use crate::state::{
    AwaitingDeal, BidProgress, Bidding, Finished, Ledger, Playing, Scoring, TrickProgress,
};
use crate::{Bid, Card, CardSet, ChanceAction, Game, ModelError, Player, PlayerAction, Pot, Score};

/// Stable rulebook source attached to semantic decisions and diffs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleOrigin {
    /// Stable rule ID.
    pub rule_id: &'static str,
    /// Source anchor in the normative Typst document.
    pub source: &'static str,
}

/// Score-cell category before presentation formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum RoundOutcome {
    /// Bid and tricks differ.
    Miss,
    /// Bid equals tricks below all tricks.
    Exact,
    /// Bid and tricks equal the round hand size.
    AllTricks,
}

/// Raw authoritative score/payment event for one player at a round boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct RoundScoreEvent {
    /// Scored player.
    pub player: Player,
    /// Fixed bid.
    pub bid: Bid,
    /// Tricks actually won.
    pub tricks_won: u8,
    /// Rulebook score-cell category.
    pub outcome: RoundOutcome,
    /// Raw points added to cumulative score.
    pub points: u8,
    /// Raw payment added to the communal pot.
    pub payment_cents: u8,
}

/// Exact final pot division, including intentionally unassigned remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct PotDivision {
    /// Number of tied winners.
    pub winner_count: u8,
    /// Cents assigned to every winner.
    pub cents_per_winner: u16,
    /// Indivisible cents left physically unspecified.
    pub remainder_cents: u16,
}

/// Final game result. Score and money remain separate semantic projections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct GameOutcome {
    /// Final cumulative points.
    pub scores: [Score; 2],
    /// Final communal pot.
    pub pot: Pot,
    /// Every seat tied for maximum score.
    pub winners: [bool; 2],
    /// Equal practical division of the pot.
    pub pot_division: PotDivision,
}

/// One source-oriented semantic state difference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticDiff {
    /// Stable field/projection path.
    pub path: &'static str,
    /// Canonical debug form before the transition.
    pub before: String,
    /// Canonical debug form after the transition.
    pub after: String,
    /// Rules responsible for this change.
    pub origins: Vec<RuleOrigin>,
}

/// Unified explicit transition action, including the sole terminal self-loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ModelAction {
    /// Explicit chance input.
    Chance(ChanceAction),
    /// Player-owned action.
    Player(PlayerAction),
    /// Deterministic round settlement.
    Settle,
    /// Total-transition self-loop admitted only by `Finished`.
    Absorb,
}

/// One strict-model transition with raw score/outcome events and provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    /// Applied explicit action.
    pub action: ModelAction,
    /// Successor state.
    pub next: Game,
    /// Raw score events present exactly at settlement.
    pub round_scores: Option<[RoundScoreEvent; 2]>,
    /// Final score/money outcome present in terminal successors.
    pub game_outcome: Option<GameOutcome>,
    /// Rules involved in the transition.
    pub origins: Vec<RuleOrigin>,
    /// Source-oriented semantic differences.
    pub diffs: Vec<SemanticDiff>,
}

impl Game {
    /// Apply one explicitly owned action using pure formal decision kernels.
    ///
    /// `Finished` admits only [`ModelAction::Absorb`]; no nonterminal transition
    /// is permitted to stutter.
    ///
    /// # Errors
    ///
    /// Returns a refined model/formal error for a wrong phase, wrong actor,
    /// illegal value, broken invariant, or failed Weavy computation.
    #[allow(
        clippy::too_many_lines,
        reason = "one exhaustive phase/action match makes transition totality auditable"
    )]
    pub fn transition(self, action: ModelAction) -> Result<Transition, ModelError> {
        self.validate()?;
        let transition = match (self, action) {
            (Self::AwaitingDeal(state), ModelAction::Chance(ChanceAction::Deal(deal))) => {
                deal.validate_round(state.ledger.round)?;
                let origins = deal_origins();
                let next = Self::Bidding(Bidding {
                    ledger: state.ledger,
                    hands: deal.hands(),
                    trump: deal.trump(),
                    undealt: deal.undealt(),
                    progress: BidProgress::First,
                });
                build_transition(
                    self,
                    action,
                    next,
                    None,
                    origins,
                    ["phase", "hands", "trump", "undealt"],
                )
            }
            (Self::Bidding(state), ModelAction::Player(PlayerAction::Bid { player, bid })) => {
                ensure_actor(state.actor(), player)?;
                if !formal::bid_legal(bid, state.ledger.round)? {
                    return Err(ModelError::BidExceedsHand {
                        bid,
                        hand_size: state.ledger.round.hand_size(),
                    });
                }
                let origins = bid_origins();
                let next = match state.progress {
                    BidProgress::First => Self::Bidding(Bidding {
                        progress: BidProgress::DealerLast(bid),
                        ..state
                    }),
                    BidProgress::DealerLast(first_bid) => {
                        let first = state.ledger.dealer.left();
                        let mut bids = [Bid::Zero; 2];
                        bids[first.index()] = first_bid;
                        bids[state.ledger.dealer.index()] = bid;
                        Self::Playing(Playing {
                            ledger: state.ledger,
                            hands: state.hands,
                            trump: state.trump,
                            undealt: state.undealt,
                            bids,
                            trick: TrickProgress::Lead(first),
                            captured: [CardSet::EMPTY; 2],
                            tricks_won: [crate::Tricks::default(); 2],
                        })
                    }
                };
                build_transition(
                    self,
                    action,
                    next,
                    None,
                    origins,
                    ["phase", "bids", "actor"],
                )
            }
            (Self::Playing(state), ModelAction::Player(PlayerAction::Play { player, card })) => {
                ensure_actor(state.actor(), player)?;
                let hand = state.hands[player.index()];
                if !hand.contains(card) {
                    return Err(ModelError::CardAbsent(card));
                }
                if let Some(lead) = state.trick.lead() {
                    let has_lead = hand.contains_suit(lead.card.suit());
                    if !formal::follow_suit_legal(has_lead, card, lead.card)? {
                        return Err(ModelError::MustFollowSuit {
                            suit: lead.card.suit(),
                        });
                    }
                }
                let mut hands = state.hands;
                hands[player.index()] = hand.without(card)?;
                let origins = play_origins();
                let next = match state.trick {
                    TrickProgress::Lead(_) => Self::Playing(Playing {
                        hands,
                        trick: TrickProgress::Follow(crate::PlayedCard { player, card }),
                        ..state
                    }),
                    TrickProgress::Follow(lead) => {
                        let winner = if formal::second_card_wins(lead.card, card, state.trump)? {
                            player
                        } else {
                            lead.player
                        };
                        let mut captured = state.captured;
                        captured[winner.index()] =
                            captured[winner.index()].with(lead.card)?.with(card)?;
                        let mut tricks_won = state.tricks_won;
                        tricks_won[winner.index()] = tricks_won[winner.index()].increment();
                        if hands.iter().all(|hand| hand.is_empty()) {
                            Self::Scoring(Scoring {
                                ledger: state.ledger,
                                trump: state.trump,
                                undealt: state.undealt,
                                bids: state.bids,
                                captured,
                                tricks_won,
                            })
                        } else {
                            Self::Playing(Playing {
                                hands,
                                trick: TrickProgress::Lead(winner),
                                captured,
                                tricks_won,
                                ..state
                            })
                        }
                    }
                };
                build_transition(
                    self,
                    action,
                    next,
                    None,
                    origins,
                    ["phase", "hands", "current-trick", "captured", "tricks-won"],
                )
            }
            (Self::Scoring(state), ModelAction::Settle) => settle(self, action, state)?,
            (Self::Finished(state), ModelAction::Absorb) => {
                let outcome = outcome(state);
                build_transition(
                    self,
                    action,
                    self,
                    Some(outcome),
                    finished_origins(),
                    std::iter::empty::<&'static str>(),
                )
            }
            _ => return Err(ModelError::WrongPhase),
        };
        transition.next.validate()?;
        if !matches!(self, Self::Finished(_)) && transition.next == self {
            return Err(ModelError::Invariant("nonterminal no-stuttering"));
        }
        Ok(transition)
    }

    /// Check cross-field refinements not eliminated solely by phase types.
    ///
    /// # Errors
    ///
    /// Returns the first violated named invariant.
    pub fn validate(self) -> Result<(), ModelError> {
        match self {
            Self::AwaitingDeal(_) => Ok(()),
            Self::Bidding(state) => {
                let hand_size = state.ledger.round.hand_size();
                if state.hands.iter().any(|hand| hand.len() != hand_size)
                    || state.undealt.len() != 5 - 2 * hand_size
                {
                    return Err(ModelError::Invariant("bidding zone cardinalities"));
                }
                validate_partition(state.hands, state.trump, state.undealt, None, &[])
            }
            Self::Playing(state) => {
                for bid in state.bids {
                    if !bid.legal_for(state.ledger.round) {
                        return Err(ModelError::Invariant("fixed bid bounds"));
                    }
                }
                let lead = state.trick.lead().map(|play| play.card);
                validate_partition(
                    state.hands,
                    state.trump,
                    state.undealt,
                    lead,
                    &state.captured,
                )?;
                let completed: u8 = state.tricks_won.iter().map(|count| count.get()).sum();
                let captured: u8 = state.captured.iter().map(|cards| cards.len()).sum();
                let current = u8::from(lead.is_some());
                let remaining: u8 = state.hands.iter().map(|cards| cards.len()).sum();
                if captured != 2 * completed
                    || remaining + current + captured != 2 * state.ledger.round.hand_size()
                {
                    return Err(ModelError::Invariant("trick/card-count conservation"));
                }
                for player in Player::ALL {
                    if state.captured[player.index()].len()
                        != 2 * state.tricks_won[player.index()].get()
                    {
                        return Err(ModelError::Invariant("captured pile ownership"));
                    }
                }
                Ok(())
            }
            Self::Scoring(state) => {
                for bid in state.bids {
                    if !bid.legal_for(state.ledger.round) {
                        return Err(ModelError::Invariant("fixed bid bounds"));
                    }
                }
                validate_partition(
                    [CardSet::EMPTY; 2],
                    state.trump,
                    state.undealt,
                    None,
                    &state.captured,
                )?;
                let completed: u8 = state.tricks_won.iter().map(|count| count.get()).sum();
                if completed != state.ledger.round.hand_size() {
                    return Err(ModelError::Invariant("round trick count"));
                }
                for player in Player::ALL {
                    if state.captured[player.index()].len()
                        != 2 * state.tricks_won[player.index()].get()
                    {
                        return Err(ModelError::Invariant("scoring captured pile ownership"));
                    }
                }
                Ok(())
            }
            Self::Finished(state) => {
                let expected = formal::winner_mask(state.scores.map(Score::get))?;
                if expected == state.winners {
                    Ok(())
                } else {
                    Err(ModelError::Invariant("final maximum-score winner mask"))
                }
            }
        }
    }

    /// Return the final score/money outcome only in `Finished`.
    #[must_use]
    pub fn terminal_outcome(self) -> Option<GameOutcome> {
        match self {
            Self::Finished(state) => Some(outcome(state)),
            _ => None,
        }
    }
}

fn settle(before: Game, action: ModelAction, state: Scoring) -> Result<Transition, ModelError> {
    let mut scores = state.ledger.scores;
    let mut pot = state.ledger.pot;
    let mut events = [RoundScoreEvent {
        player: Player::Zero,
        bid: Bid::Zero,
        tricks_won: 0,
        outcome: RoundOutcome::Miss,
        points: 0,
        payment_cents: 0,
    }; 2];
    for player in Player::ALL {
        let bid = state.bids[player.index()];
        let tricks = state.tricks_won[player.index()].get();
        let (outcome, points, payment_cents) =
            formal::round_score(bid, tricks, state.ledger.round)?;
        let outcome = match outcome {
            0 => RoundOutcome::Miss,
            1 => RoundOutcome::Exact,
            2 => RoundOutcome::AllTricks,
            _ => return Err(ModelError::Invariant("round score outcome ordinal")),
        };
        scores[player.index()] = scores[player.index()].add(points)?;
        if payment_cents != 0 {
            if payment_cents != 10 {
                return Err(ModelError::Invariant("missed-payment amount"));
            }
            pot = pot.add_miss()?;
        }
        events[player.index()] = RoundScoreEvent {
            player,
            bid,
            tricks_won: tricks,
            outcome,
            points,
            payment_cents,
        };
    }
    let final_round = formal::final_round(state.ledger.round)?;
    let next = if final_round {
        let winners = formal::winner_mask(scores.map(Score::get))?;
        Game::Finished(Finished {
            scores,
            pot,
            winners,
        })
    } else {
        let round = state
            .ledger
            .round
            .next()
            .ok_or(ModelError::Invariant("nonfinal round has successor"))?;
        Game::AwaitingDeal(AwaitingDeal {
            ledger: Ledger {
                dealer: state.ledger.dealer.left(),
                round,
                scores,
                pot,
            },
        })
    };
    let game_outcome = next.terminal_outcome();
    let mut transition = build_transition(
        before,
        action,
        next,
        game_outcome,
        score_origins(),
        ["phase", "round", "dealer", "scores", "pot"],
    );
    transition.round_scores = Some(events);
    Ok(transition)
}

fn outcome(state: Finished) -> GameOutcome {
    let winner_count =
        u8::try_from(state.winners.into_iter().filter(|winner| *winner).count()).unwrap_or(2);
    let pot_cents = state.pot.cents();
    let divisor = u16::from(winner_count);
    GameOutcome {
        scores: state.scores,
        pot: state.pot,
        winners: state.winners,
        pot_division: PotDivision {
            winner_count,
            cents_per_winner: pot_cents / divisor,
            remainder_cents: pot_cents % divisor,
        },
    }
}

fn build_transition<I>(
    before: Game,
    action: ModelAction,
    next: Game,
    game_outcome: Option<GameOutcome>,
    origins: Vec<RuleOrigin>,
    paths: I,
) -> Transition
where
    I: IntoIterator<Item = &'static str>,
{
    let diffs = paths
        .into_iter()
        .filter_map(|path| semantic_diff(path, before, next, &origins))
        .collect();
    Transition {
        action,
        next,
        round_scores: None,
        game_outcome,
        origins,
        diffs,
    }
}

fn semantic_diff(
    path: &'static str,
    before: Game,
    after: Game,
    origins: &[RuleOrigin],
) -> Option<SemanticDiff> {
    let (before, after) = match path {
        "phase" => (debug(before.phase()), debug(after.phase())),
        "round" => (debug(round_of(before)), debug(round_of(after))),
        "dealer" => (debug(dealer_of(before)), debug(dealer_of(after))),
        "actor" => (debug(before.turn()), debug(after.turn())),
        "hands" => (debug(hands_of(before)), debug(hands_of(after))),
        "trump" => (debug(trump_of(before)), debug(trump_of(after))),
        "undealt" => (debug(undealt_of(before)), debug(undealt_of(after))),
        "bids" => (debug(bids_of(before)), debug(bids_of(after))),
        "current-trick" => (debug(trick_of(before)), debug(trick_of(after))),
        "captured" => (debug(captured_of(before)), debug(captured_of(after))),
        "tricks-won" => (debug(tricks_of(before)), debug(tricks_of(after))),
        "scores" => (debug(scores_of(before)), debug(scores_of(after))),
        "pot" => (debug(pot_of(before)), debug(pot_of(after))),
        _ => return None,
    };
    (before != after).then(|| SemanticDiff {
        path,
        before,
        after,
        origins: origins.to_vec(),
    })
}

fn validate_partition(
    hands: [CardSet; 2],
    trump: Card,
    undealt: CardSet,
    current: Option<Card>,
    captured: &[CardSet],
) -> Result<(), ModelError> {
    let mut zones = vec![hands[0], hands[1], undealt];
    zones.extend_from_slice(captured);
    if let Some(card) = current {
        zones.push(CardSet::EMPTY.with(card)?);
    }
    zones.push(CardSet::EMPTY.with(trump)?);
    let mut all = CardSet::EMPTY;
    for zone in zones {
        all = all.union(zone)?;
    }
    if all.len() == 6 {
        Ok(())
    } else {
        Err(ModelError::IncompletePartition)
    }
}

fn ensure_actor(expected: Player, actual: Player) -> Result<(), ModelError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ModelError::WrongActor { expected, actual })
    }
}

fn debug(value: impl fmt::Debug) -> String {
    format!("{value:?}")
}

fn round_of(game: Game) -> Option<crate::RoundId> {
    ledger_of(game).map(Ledger::round)
}

fn dealer_of(game: Game) -> Option<Player> {
    ledger_of(game).map(Ledger::dealer)
}

fn ledger_of(game: Game) -> Option<Ledger> {
    match game {
        Game::AwaitingDeal(state) => Some(state.ledger),
        Game::Bidding(state) => Some(state.ledger),
        Game::Playing(state) => Some(state.ledger),
        Game::Scoring(state) => Some(state.ledger),
        Game::Finished(_) => None,
    }
}

fn hands_of(game: Game) -> Option<[CardSet; 2]> {
    match game {
        Game::Bidding(state) => Some(state.hands),
        Game::Playing(state) => Some(state.hands),
        _ => None,
    }
}

fn trump_of(game: Game) -> Option<Card> {
    match game {
        Game::Bidding(state) => Some(state.trump),
        Game::Playing(state) => Some(state.trump),
        Game::Scoring(state) => Some(state.trump),
        _ => None,
    }
}

fn undealt_of(game: Game) -> Option<CardSet> {
    match game {
        Game::Bidding(state) => Some(state.undealt),
        Game::Playing(state) => Some(state.undealt),
        Game::Scoring(state) => Some(state.undealt),
        _ => None,
    }
}

fn bids_of(game: Game) -> Option<[Option<Bid>; 2]> {
    match game {
        Game::Bidding(state) => Some(state.bids()),
        Game::Playing(state) => Some(state.bids.map(Some)),
        Game::Scoring(state) => Some(state.bids.map(Some)),
        _ => None,
    }
}

fn trick_of(game: Game) -> Option<crate::PlayedCard> {
    match game {
        Game::Playing(state) => state.trick.lead(),
        _ => None,
    }
}

fn captured_of(game: Game) -> Option<[CardSet; 2]> {
    match game {
        Game::Playing(state) => Some(state.captured),
        Game::Scoring(state) => Some(state.captured),
        _ => None,
    }
}

fn tricks_of(game: Game) -> Option<[crate::Tricks; 2]> {
    match game {
        Game::Playing(state) => Some(state.tricks_won),
        Game::Scoring(state) => Some(state.tricks_won),
        _ => None,
    }
}

fn scores_of(game: Game) -> [Score; 2] {
    match game {
        Game::AwaitingDeal(state) => state.ledger.scores,
        Game::Bidding(state) => state.ledger.scores,
        Game::Playing(state) => state.ledger.scores,
        Game::Scoring(state) => state.ledger.scores,
        Game::Finished(state) => state.scores,
    }
}

fn pot_of(game: Game) -> Pot {
    match game {
        Game::AwaitingDeal(state) => state.ledger.pot,
        Game::Bidding(state) => state.ledger.pot,
        Game::Playing(state) => state.ledger.pot,
        Game::Scoring(state) => state.ledger.pot,
        Game::Finished(state) => state.pot,
    }
}

fn origin(rule_id: &'static str, source: &'static str) -> RuleOrigin {
    RuleOrigin { rule_id, source }
}

fn deal_origins() -> Vec<RuleOrigin> {
    vec![
        origin("R-DEAL-001", "docs/main.typ:261-265"),
        origin("R-DEAL-002", "docs/main.typ:265,417-431"),
        origin("R-DEAL-003", "docs/main.typ:267-269"),
        origin("R-DEAL-004", "docs/main.typ:270"),
    ]
}

fn bid_origins() -> Vec<RuleOrigin> {
    vec![
        origin("R-BID-001", "docs/main.typ:273-280"),
        origin("R-BID-002", "docs/main.typ:275-277"),
        origin("R-BID-003", "docs/main.typ:278"),
        origin("R-BID-004", "docs/main.typ:279"),
        origin("R-BID-005", "docs/main.typ:280"),
    ]
}

fn play_origins() -> Vec<RuleOrigin> {
    vec![
        origin("R-TRICK-001", "docs/main.typ:282-284"),
        origin("R-TRICK-002", "docs/main.typ:286-288"),
        origin("R-TRICK-003", "docs/main.typ:288"),
        origin("R-TRICK-004", "docs/main.typ:290"),
        origin("R-TRICK-005", "docs/main.typ:292-294"),
        origin("R-TRICK-006", "docs/main.typ:294"),
        origin("R-TRICK-007", "docs/main.typ:296-302"),
        origin("R-TRICK-008", "docs/main.typ:296-302"),
        origin("R-TRICK-009", "docs/main.typ:300"),
        origin("R-TRICK-010", "docs/main.typ:301"),
        origin("R-TRICK-011", "docs/main.typ:302"),
        origin("R-TRICK-012", "docs/main.typ:304"),
    ]
}

fn score_origins() -> Vec<RuleOrigin> {
    vec![
        origin("R-SCORE-001", "docs/main.typ:311-313"),
        origin("R-SCORE-002", "docs/main.typ:317-323"),
        origin("R-SCORE-003", "docs/main.typ:317-323"),
        origin("R-SCORE-004", "docs/main.typ:317-323"),
        origin("R-SCORE-005", "docs/main.typ:323"),
        origin("R-MONEY-001", "docs/main.typ:350-353"),
        origin("R-MONEY-002", "docs/main.typ:354"),
        origin("R-ADVANCE-001", "docs/main.typ:357-363"),
        origin("R-ADVANCE-002", "docs/main.typ:362"),
        origin("R-ADVANCE-003", "docs/main.typ:363"),
        origin("R-FINISH-001", "docs/main.typ:365-368"),
        origin("R-FINISH-002", "docs/main.typ:368"),
        origin("R-FINISH-003", "docs/main.typ:369"),
        origin("R-FINISH-004", "docs/main.typ:370"),
    ]
}

fn finished_origins() -> Vec<RuleOrigin> {
    vec![
        origin("R-GAME-003", "docs/main.typ:46-52"),
        origin("R-FINISH-001", "docs/main.typ:365-368"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Deal, LegalActions, Phase, RoundId, TurnOwner, formal_kernel_catalog};

    fn action_bid(game: Game, bid: Bid) -> ModelAction {
        let TurnOwner::Player(player) = game.turn() else {
            panic!("expected player turn");
        };
        ModelAction::Player(PlayerAction::Bid { player, bid })
    }

    fn action_play(game: Game, card: Card) -> ModelAction {
        let TurnOwner::Player(player) = game.turn() else {
            panic!("expected player turn");
        };
        ModelAction::Player(PlayerAction::Play { player, card })
    }

    fn enter_playing(deal: Deal, first_bid: Bid, dealer_bid: Bid) -> Game {
        let round = if deal.hand_size() == 1 {
            RoundId::OneAscending
        } else {
            RoundId::Two
        };
        let game = Game::AwaitingDeal(AwaitingDeal {
            ledger: Ledger {
                dealer: Player::One,
                round,
                scores: [Score::ZERO; 2],
                pot: Pot::OPENING,
            },
        })
        .transition(ModelAction::Chance(ChanceAction::Deal(deal)))
        .unwrap()
        .next;
        let game = game.transition(action_bid(game, first_bid)).unwrap().next;
        game.transition(action_bid(game, dealer_bid)).unwrap().next
    }

    #[test]
    fn transition_examples_follow_phase_and_actor_structure() {
        let kernels = formal_kernel_catalog().unwrap();
        assert_eq!(kernels.len(), 6);
        assert!(kernels.iter().all(|kernel| kernel.instruction_count > 0));

        let initial = Game::new(Player::One);
        let deal = Deal::all_for(RoundId::OneAscending)[0];
        let dealt = initial
            .transition(ModelAction::Chance(ChanceAction::Deal(deal)))
            .unwrap();
        assert_eq!(dealt.next.phase(), Phase::Bidding);
        assert_eq!(dealt.next.turn(), TurnOwner::Player(Player::Zero));
        assert!(
            dealt
                .origins
                .iter()
                .any(|origin| origin.rule_id == "R-DEAL-003")
        );
        assert!(dealt.diffs.iter().any(|diff| diff.path == "hands"));

        let first_bid = dealt
            .next
            .transition(action_bid(dealt.next, Bid::Zero))
            .unwrap();
        assert_eq!(first_bid.next.phase(), Phase::Bidding);
        assert_eq!(first_bid.next.turn(), TurnOwner::Player(Player::One));
        let playing = first_bid
            .next
            .transition(action_bid(first_bid.next, Bid::One))
            .unwrap();
        assert_eq!(playing.next.phase(), Phase::Playing);
        assert_eq!(playing.next.turn(), TurnOwner::Player(Player::Zero));
        assert!(
            playing
                .origins
                .iter()
                .any(|origin| origin.rule_id == "R-BID-004")
        );
        playing.next.validate().unwrap();

        assert!(formal::second_card_wins(Card::S0R2, Card::S1R0, Card::S1R2).unwrap());
        assert!(formal::second_card_wins(Card::S0R0, Card::S0R2, Card::S1R0).unwrap());
        assert!(!formal::second_card_wins(Card::S0R2, Card::S1R2, Card::S0R0).unwrap());
    }

    #[test]
    fn follow_suit_examples_are_computed_by_the_formal_kernel() {
        let (deal, lead, legal_follow, illegal_follow) = Deal::all_for(RoundId::Two)
            .into_iter()
            .find_map(|deal| {
                let leader_hand = deal.hands()[Player::Zero.index()];
                let follower_hand = deal.hands()[Player::One.index()];
                leader_hand.iter().find_map(|lead| {
                    let legal = follower_hand
                        .iter()
                        .find(|card| card.suit() == lead.suit())?;
                    let illegal = follower_hand
                        .iter()
                        .find(|card| card.suit() != lead.suit())?;
                    Some((deal, lead, legal, illegal))
                })
            })
            .expect("micro deals contain a discriminating follow-suit case");
        let game = enter_playing(deal, Bid::Zero, Bid::Zero);
        let game = game.transition(action_play(game, lead)).unwrap().next;
        let LegalActions::Player(actions) = game.legal_actions() else {
            panic!("expected player actions");
        };
        assert_eq!(
            actions,
            vec![PlayerAction::Play {
                player: Player::One,
                card: legal_follow,
            }]
        );
        assert!(matches!(
            game.transition(action_play(game, illegal_follow)),
            Err(ModelError::MustFollowSuit { .. })
        ));
        game.transition(action_play(game, legal_follow)).unwrap();
    }

    #[test]
    fn scoring_examples_cover_miss_exact_and_all_tricks() {
        let cases = [
            (
                Bid::Zero,
                0,
                RoundId::OneAscending,
                RoundOutcome::Exact,
                10,
                0,
            ),
            (Bid::One, 1, RoundId::Two, RoundOutcome::Exact, 11, 0),
            (Bid::Two, 2, RoundId::Two, RoundOutcome::AllTricks, 22, 0),
            (Bid::One, 0, RoundId::Two, RoundOutcome::Miss, 0, 10),
        ];
        for (bid, tricks, round, expected_outcome, points, payment) in cases {
            let (outcome, actual_points, actual_payment) =
                formal::round_score(bid, tricks, round).unwrap();
            let actual_outcome = [
                RoundOutcome::Miss,
                RoundOutcome::Exact,
                RoundOutcome::AllTricks,
            ][usize::from(outcome)];
            assert_eq!(actual_outcome, expected_outcome);
            assert_eq!(actual_points, points);
            assert_eq!(actual_payment, payment);
        }

        // The first one-card deal is 0 vs 1 with trump 2, so player one wins.
        let deal = Deal::all_for(RoundId::OneAscending)[0];
        let game = enter_playing(deal, Bid::Zero, Bid::One);
        let p0_card = deal.hands()[0].iter().next().unwrap();
        let p1_card = deal.hands()[1].iter().next().unwrap();
        let game = game.transition(action_play(game, p0_card)).unwrap().next;
        let scoring = game.transition(action_play(game, p1_card)).unwrap().next;
        assert_eq!(scoring.phase(), Phase::Scoring);
        let settled = scoring.transition(ModelAction::Settle).unwrap();
        let events = settled.round_scores.unwrap();
        assert_eq!(events[0].outcome, RoundOutcome::Exact);
        assert_eq!(events[0].points, 10);
        assert_eq!(events[1].outcome, RoundOutcome::AllTricks);
        assert_eq!(events[1].points, 21);
        assert_eq!(settled.next.phase(), Phase::AwaitingDeal);
    }

    #[test]
    fn terminal_and_scoring_complete_the_three_round_schedule() {
        let mut game = Game::new(Player::One);
        let mut scored_rounds = Vec::new();
        let mut steps = 0_u32;
        while game.phase() != Phase::Finished {
            let action = match game.legal_actions() {
                LegalActions::Chance(actions) => ModelAction::Chance(actions[0]),
                LegalActions::Player(actions) => ModelAction::Player(actions[0]),
                LegalActions::Environment => ModelAction::Settle,
                LegalActions::Finished => unreachable!(),
            };
            let transition = game.transition(action).unwrap();
            if let Some(events) = transition.round_scores {
                scored_rounds.push(events);
            }
            game = transition.next;
            steps += 1;
            assert!(steps < 100);
        }
        assert_eq!(scored_rounds.len(), 3);
        assert!(
            scored_rounds
                .iter()
                .all(|events| events.iter().all(|event| event.points <= 22))
        );
        let outcome = game.terminal_outcome().unwrap();
        assert_eq!(outcome.pot.cents(), 80);
        assert_eq!(
            u8::try_from(outcome.winners.into_iter().filter(|winner| *winner).count()).unwrap(),
            outcome.pot_division.winner_count
        );

        let absorbed = game.transition(ModelAction::Absorb).unwrap();
        assert_eq!(absorbed.next, game);
        assert!(absorbed.diffs.is_empty());
        assert!(game.transition(ModelAction::Settle).is_err());
    }
}

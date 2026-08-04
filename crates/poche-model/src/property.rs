use facet::Facet;

use crate::formal;
use crate::{
    Game, KernelId, LegalActions, ModelAction, ModelError, Phase, Player, PlayerAction, RuleOrigin,
    Transition, TrickProgress, TurnOwner,
};

/// Stable strict-model property identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum PropertyId {
    /// Every card occupies exactly one semantic zone.
    CardConservation,
    /// Action owner and enumerated action payload agree.
    LegalActor,
    /// Viewer observations contain only their own private hand.
    ObservationConfidentiality,
    /// Announced bids stay fixed through play and scoring.
    FixedBids,
    /// Followers holding lead suit must follow it.
    FollowSuit,
    /// Complete tricks have exactly one deterministic eligible winner.
    TrickWinner,
    /// The trick winner leads the next trick.
    WinnerLeads,
    /// Trick credits and captured piles agree.
    TrickCountConservation,
    /// Settlement points and payments follow the score table.
    ScoringAndPot,
    /// Nonterminal transitions make phase-local progress.
    PhaseProgress,
    /// Every state has a total successor; only `Finished` stutters.
    DeadlockFreedom,
    /// Every path from every prepared initial state reaches `Finished`.
    UniversalTermination,
    /// `Finished` is absorbing and has no gameplay action.
    FinishedAbsorbing,
    /// Winners are exactly all maximum-score players.
    FinalWinnerSemantics,
}

/// Whether a claim is eliminated structurally or requires exploration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyClass {
    /// Guaranteed by closed Rust type shape/private construction.
    Structural,
    /// Checked in every reachable state.
    ReachableState,
    /// Checked over every reachable transition.
    Transition,
    /// Checked over the complete reachable graph.
    Temporal,
}

/// Inspectable computation or temporal formula defining a claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyExpression {
    /// A closed type-shape/refinement assertion.
    Structural(&'static str),
    /// A pure Weavy kernel.
    Kernel(KernelId),
    /// Named executable state predicate.
    StatePredicate(&'static str),
    /// Named executable transition predicate.
    TransitionPredicate(&'static str),
    /// Graph/temporal formula evaluated by explicit-state and native tools.
    Temporal(&'static str),
}

/// Planned/implemented method establishing a property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStrategy {
    /// Rust type construction is sufficient.
    Structural,
    /// Exhaust every finite input combination to a pure kernel.
    KernelExhaustive,
    /// Evaluate over every reachable state in the named scope.
    ReachableStateExhaustive,
    /// Evaluate over every reachable transition in the named scope.
    TransitionExhaustive,
    /// Detect deadlocks in the explicit graph.
    DeadlockSearch,
    /// Compute nonterminal SCCs and prefix/cycle lassos.
    SccTermination,
}

/// One equivalent/native check used for cross-model agreement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCheckRef {
    /// Backend family.
    pub backend: &'static str,
    /// Stable assertion/query/property name in that backend.
    pub check_id: &'static str,
}

/// Complete declaration of one safety, consistency, or liveness claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertySpec {
    /// Stable property identity.
    pub id: PropertyId,
    /// Stable kebab-case interchange ID.
    pub stable_id: &'static str,
    /// Structural/state/transition/temporal classification.
    pub class: PropertyClass,
    /// Inspectable formal expression.
    pub expression: PropertyExpression,
    /// Intended verification method.
    pub strategy: VerificationStrategy,
    /// Human explanation of the semantic claim.
    pub explanation: &'static str,
    /// Normative rule origins.
    pub rules: Vec<RuleOrigin>,
    /// Equivalent checks in independent native models.
    pub native_checks: Vec<NativeCheckRef>,
}

/// Return the complete strict-model property catalog.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the complete declarative catalog is intentionally visible in one audit surface"
)]
pub fn property_catalog() -> Vec<PropertySpec> {
    vec![
        spec(
            PropertyId::CardConservation,
            "card-conservation",
            PropertyClass::ReachableState,
            PropertyExpression::StatePredicate("state_card_partition"),
            VerificationStrategy::ReachableStateExhaustive,
            "Each of six cards occurs in exactly one hand, trump, undealt, current-trick, or captured zone.",
            &[
                ("R-GAME-002", "docs/main.typ:54-62,301-302"),
                ("R-DEAL-004", "docs/main.typ:270"),
                ("R-TRICK-012", "docs/main.typ:304"),
                ("R-ADVANCE-001", "docs/main.typ:357-363"),
            ],
            &[
                ("alloy", "CardConservationAndPartition"),
                ("nusmv", "card_total"),
                ("prolog", "valid_round_state/1"),
            ],
        ),
        spec(
            PropertyId::LegalActor,
            "legal-actor",
            PropertyClass::ReachableState,
            PropertyExpression::StatePredicate("state_legal_actor_and_actions"),
            VerificationStrategy::ReachableStateExhaustive,
            "Every enumerated player action belongs to the unique acting seat; chance and settlement remain disjoint.",
            &[
                ("R-GAME-003", "docs/main.typ:46-52"),
                ("R-GAME-004", "docs/main.typ:70-79"),
                ("R-BID-001", "docs/main.typ:273-280"),
                ("R-TRICK-004", "docs/main.typ:290"),
            ],
            &[
                ("alloy", "DealerBidsLastInClockwiseOrder"),
                ("nusmv", "actor invariants"),
                ("prolog", "legal_action/2"),
            ],
        ),
        spec(
            PropertyId::ObservationConfidentiality,
            "observation-confidentiality",
            PropertyClass::Structural,
            PropertyExpression::Structural(
                "Observation has one private_hand field indexed only by viewer",
            ),
            VerificationStrategy::Structural,
            "A player's observation carries that player's cards and only public counts/cards from every other zone.",
            &[
                ("R-GAME-004", "docs/main.typ:70-79"),
                ("R-BID-003", "docs/main.typ:278"),
                ("R-TRICK-004", "docs/main.typ:290"),
            ],
            &[
                (
                    "rust-oracle",
                    "replay_is_deterministic_and_observation_hides_other_hands",
                ),
                ("shared", "private-observation-does-not-leak"),
            ],
        ),
        spec(
            PropertyId::FixedBids,
            "fixed-bids",
            PropertyClass::Transition,
            PropertyExpression::TransitionPredicate("transition_preserves_announced_bids"),
            VerificationStrategy::TransitionExhaustive,
            "Once announced, each bid remains unchanged through every play and scoring state.",
            &[
                ("R-BID-002", "docs/main.typ:275-277"),
                ("R-BID-003", "docs/main.typ:278"),
            ],
            &[
                ("alloy", "immutable Round.bid snapshot"),
                ("nusmv", "bid persistence invariants"),
                ("prolog", "immutable bid list"),
            ],
        ),
        spec(
            PropertyId::FollowSuit,
            "follow-suit",
            PropertyClass::Transition,
            PropertyExpression::Kernel(KernelId::FollowSuitLegal),
            VerificationStrategy::KernelExhaustive,
            "A follower with lead suit may select only lead-suit cards; a void follower may select any hand card.",
            &[
                ("R-TRICK-005", "docs/main.typ:292-294"),
                ("R-TRICK-006", "docs/main.typ:294"),
            ],
            &[
                ("alloy", "FollowSuitIsEnforced"),
                ("nusmv", "follow-suit TRANS"),
                ("prolog", "follow_suit corpus"),
            ],
        ),
        spec(
            PropertyId::TrickWinner,
            "trick-winner",
            PropertyClass::Transition,
            PropertyExpression::Kernel(KernelId::SecondCardWins),
            VerificationStrategy::KernelExhaustive,
            "Trump eligibility dominates lead-suit eligibility, then rank uniquely determines the two-card trick winner.",
            &[
                ("R-TRICK-007", "docs/main.typ:296-302"),
                ("R-TRICK-008", "docs/main.typ:296-302"),
                ("R-TRICK-009", "docs/main.typ:300"),
                ("R-TRICK-010", "docs/main.typ:301"),
                ("R-TRICK-011", "docs/main.typ:302"),
            ],
            &[
                ("alloy", "WinnerIsEligibleAndHighest"),
                ("nusmv", "expected_winner"),
                ("prolog", "card_strength/5"),
            ],
        ),
        spec(
            PropertyId::WinnerLeads,
            "winner-leads-next-trick",
            PropertyClass::Transition,
            PropertyExpression::TransitionPredicate("transition_winner_becomes_leader"),
            VerificationStrategy::TransitionExhaustive,
            "The resolved trick winner receives the cards/credit and leads the next trick when one remains.",
            &[
                ("R-TRICK-002", "docs/main.typ:286-288"),
                ("R-TRICK-012", "docs/main.typ:304"),
            ],
            &[
                ("alloy", "TrickWinnerAndCapture"),
                ("nusmv", "winner-leads CTL"),
                ("prolog", "collect_cards/4"),
            ],
        ),
        spec(
            PropertyId::TrickCountConservation,
            "trick-count-conservation",
            PropertyClass::ReachableState,
            PropertyExpression::StatePredicate("state_trick_credit_matches_captured_cards"),
            VerificationStrategy::ReachableStateExhaustive,
            "Each completed trick contributes two captured cards and exactly one trick credit.",
            &[
                ("R-TRICK-001", "docs/main.typ:282-284"),
                ("R-TRICK-012", "docs/main.typ:304"),
            ],
            &[
                ("alloy", "TrickWinnerAndCapture"),
                ("nusmv", "captured counts"),
                ("prolog", "valid_round_state/1"),
            ],
        ),
        spec(
            PropertyId::ScoringAndPot,
            "scoring-and-pot",
            PropertyClass::Transition,
            PropertyExpression::Kernel(KernelId::RoundScore),
            VerificationStrategy::KernelExhaustive,
            "Round points, missed-bid payment, cumulative score, and pot updates agree while score and money stay distinct.",
            &[
                ("R-SCORE-001", "docs/main.typ:311-313"),
                ("R-SCORE-002", "docs/main.typ:317-323"),
                ("R-SCORE-003", "docs/main.typ:317-323"),
                ("R-SCORE-004", "docs/main.typ:317-323"),
                ("R-SCORE-005", "docs/main.typ:323"),
                ("R-MONEY-001", "docs/main.typ:350-353"),
                ("R-MONEY-002", "docs/main.typ:354"),
            ],
            &[
                ("alloy", "ScoreAndPaymentAgree"),
                ("nusmv", "expected_round_score"),
                ("prolog", "round_score/6"),
            ],
        ),
        spec(
            PropertyId::PhaseProgress,
            "phase-progress",
            PropertyClass::Transition,
            PropertyExpression::TransitionPredicate("nonterminal_transition_changes_state"),
            VerificationStrategy::TransitionExhaustive,
            "Every nonterminal action consumes a bid/card/round boundary; no nonterminal self-loop exists.",
            &[
                ("R-GAME-003", "docs/main.typ:46-52"),
                ("R-ADVANCE-003", "docs/main.typ:363"),
            ],
            &[
                ("alloy", "PhaseTraceShape"),
                ("nusmv", "phase CTL properties"),
                ("prolog", "step/3"),
            ],
        ),
        spec(
            PropertyId::DeadlockFreedom,
            "deadlock-freedom",
            PropertyClass::Temporal,
            PropertyExpression::Temporal("AG EX TRUE; only Finished uses the absorbing edge"),
            VerificationStrategy::DeadlockSearch,
            "Every reachable nonterminal state has at least one legal transition and Finished has its explicit absorbing edge.",
            &[("R-GAME-003", "docs/main.typ:46-52")],
            &[
                ("nusmv", "AG EX TRUE"),
                ("rust-explicit", "nonterminal deadlock search"),
            ],
        ),
        spec(
            PropertyId::UniversalTermination,
            "universal-termination",
            PropertyClass::Temporal,
            PropertyExpression::Temporal(
                "AF Finished from every prepared initial state, without fairness",
            ),
            VerificationStrategy::SccTermination,
            "No reachable nonterminal strongly connected component can avoid Finished forever.",
            &[
                ("R-ADVANCE-003", "docs/main.typ:363"),
                ("R-FINISH-001", "docs/main.typ:365-368"),
            ],
            &[
                ("nusmv", "AF finished"),
                ("rust-explicit", "nonterminal SCC search"),
            ],
        ),
        spec(
            PropertyId::FinishedAbsorbing,
            "finished-absorbing",
            PropertyClass::Transition,
            PropertyExpression::TransitionPredicate("finished_has_only_absorb_self_loop"),
            VerificationStrategy::TransitionExhaustive,
            "Finished has exactly its total-transition self-loop and rejects every gameplay action.",
            &[
                ("R-GAME-003", "docs/main.typ:46-52"),
                ("R-FINISH-001", "docs/main.typ:365-368"),
            ],
            &[
                ("nusmv", "absorbing terminal state"),
                ("rust-explicit", "finished successor check"),
            ],
        ),
        spec(
            PropertyId::FinalWinnerSemantics,
            "final-winner-semantics",
            PropertyClass::ReachableState,
            PropertyExpression::Kernel(KernelId::WinnerMask),
            VerificationStrategy::KernelExhaustive,
            "Every and only maximum-score seat is marked a winner; ties remain shared victories.",
            &[
                ("R-GAME-005", "docs/main.typ:64-68"),
                ("R-FINISH-003", "docs/main.typ:369"),
                ("R-FINISH-004", "docs/main.typ:370"),
            ],
            &[
                ("alloy", "FinalWinnersAreExactlyTheMaxima"),
                ("nusmv", "winner maximum definitions"),
                ("prolog", "game_winners/2"),
            ],
        ),
    ]
}

/// Evaluate a catalog state predicate where locally applicable.
///
/// Temporal properties return their executable local obligation (successor
/// existence) but require graph exploration for the catalog claim.
///
/// # Errors
///
/// Returns a model/formal error if evaluating an underlying refinement fails.
pub fn evaluate_state_property(id: PropertyId, state: Game) -> Result<bool, ModelError> {
    match id {
        PropertyId::CardConservation
        | PropertyId::TrickCountConservation
        | PropertyId::FinalWinnerSemantics => Ok(state.validate().is_ok()),
        PropertyId::LegalActor => Ok(actions_match_owner(state)),
        PropertyId::ObservationConfidentiality => Ok(Player::ALL
            .into_iter()
            .all(|viewer| state.observe(viewer).private_hand == private_hand(state, viewer))),
        PropertyId::DeadlockFreedom => Ok(match state.legal_actions() {
            LegalActions::Chance(actions) => !actions.is_empty(),
            LegalActions::Player(actions) => !actions.is_empty(),
            LegalActions::Environment | LegalActions::Finished => true,
        }),
        PropertyId::FinishedAbsorbing => Ok(if state.phase() == Phase::Finished {
            state.transition(ModelAction::Absorb)?.next == state
        } else {
            true
        }),
        PropertyId::FixedBids
        | PropertyId::FollowSuit
        | PropertyId::TrickWinner
        | PropertyId::WinnerLeads
        | PropertyId::ScoringAndPot
        | PropertyId::PhaseProgress
        | PropertyId::UniversalTermination => Ok(true),
    }
}

/// Evaluate a catalog transition predicate where locally applicable.
///
/// # Errors
///
/// Returns a model/formal error if recomputing a pure semantic decision fails.
pub fn evaluate_transition_property(
    id: PropertyId,
    before: Game,
    transition: &Transition,
) -> Result<bool, ModelError> {
    let after = transition.next;
    match id {
        PropertyId::CardConservation | PropertyId::TrickCountConservation => {
            Ok(before.validate().is_ok() && after.validate().is_ok())
        }
        PropertyId::LegalActor => Ok(action_matches_turn(transition.action, before.turn())),
        PropertyId::ObservationConfidentiality => {
            evaluate_state_property(id, before).and_then(|before_ok| {
                evaluate_state_property(id, after).map(|after_ok| before_ok && after_ok)
            })
        }
        PropertyId::FixedBids => Ok(fixed_bids_preserved(before, after)),
        PropertyId::FollowSuit => follow_suit_transition(before, transition),
        PropertyId::TrickWinner | PropertyId::WinnerLeads => {
            winner_transition(id, before, transition)
        }
        PropertyId::ScoringAndPot => scoring_transition(before, transition),
        PropertyId::PhaseProgress => Ok(if before.phase() == Phase::Finished {
            after == before
        } else {
            after != before
        }),
        PropertyId::DeadlockFreedom | PropertyId::UniversalTermination => Ok(true),
        PropertyId::FinishedAbsorbing => Ok(if before.phase() == Phase::Finished {
            transition.action == ModelAction::Absorb && after == before
        } else {
            transition.action != ModelAction::Absorb
        }),
        PropertyId::FinalWinnerSemantics => evaluate_state_property(id, after),
    }
}

fn actions_match_owner(state: Game) -> bool {
    match (state.turn(), state.legal_actions()) {
        (TurnOwner::Chance, LegalActions::Chance(actions)) => !actions.is_empty(),
        (TurnOwner::Player(player), LegalActions::Player(actions)) => {
            !actions.is_empty()
                && actions.iter().all(|action| match action {
                    PlayerAction::Bid { player: actor, .. }
                    | PlayerAction::Play { player: actor, .. } => *actor == player,
                })
        }
        (TurnOwner::Environment, LegalActions::Environment)
        | (TurnOwner::Finished, LegalActions::Finished) => true,
        _ => false,
    }
}

fn action_matches_turn(action: ModelAction, owner: TurnOwner) -> bool {
    match (action, owner) {
        (ModelAction::Chance(_), TurnOwner::Chance)
        | (ModelAction::Settle, TurnOwner::Environment)
        | (ModelAction::Absorb, TurnOwner::Finished) => true,
        (
            ModelAction::Player(
                PlayerAction::Bid { player, .. } | PlayerAction::Play { player, .. },
            ),
            TurnOwner::Player(actor),
        ) => player == actor,
        _ => false,
    }
}

fn private_hand(state: Game, viewer: Player) -> crate::CardSet {
    match state {
        Game::Bidding(state) => state.hands[viewer.index()],
        Game::Playing(state) => state.hands[viewer.index()],
        _ => crate::CardSet::EMPTY,
    }
}

fn fixed_bids_preserved(before: Game, after: Game) -> bool {
    if matches!(before, Game::Scoring(_)) {
        return true;
    }
    let before = bids(before);
    let after = bids(after);
    before
        .into_iter()
        .zip(after)
        .all(|(old, new)| old.is_none() || old == new)
}

fn bids(state: Game) -> [Option<crate::Bid>; 2] {
    match state {
        Game::Bidding(state) => state.bids(),
        Game::Playing(state) => state.bids.map(Some),
        Game::Scoring(state) => state.bids.map(Some),
        _ => [None, None],
    }
}

fn follow_suit_transition(before: Game, transition: &Transition) -> Result<bool, ModelError> {
    let (Game::Playing(state), ModelAction::Player(PlayerAction::Play { player, card })) =
        (before, transition.action)
    else {
        return Ok(true);
    };
    let Some(lead) = state.trick.lead() else {
        return Ok(true);
    };
    formal::follow_suit_legal(
        state.hands[player.index()].contains_suit(lead.card.suit()),
        card,
        lead.card,
    )
}

fn winner_transition(
    id: PropertyId,
    before: Game,
    transition: &Transition,
) -> Result<bool, ModelError> {
    let (Game::Playing(state), ModelAction::Player(PlayerAction::Play { player, card })) =
        (before, transition.action)
    else {
        return Ok(true);
    };
    let TrickProgress::Follow(lead) = state.trick else {
        return Ok(true);
    };
    let winner = if formal::second_card_wins(lead.card, card, state.trump)? {
        player
    } else {
        lead.player
    };
    if id == PropertyId::TrickWinner {
        let before_count = state.tricks_won[winner.index()].get();
        let after_count = match transition.next {
            Game::Playing(after) => after.tricks_won[winner.index()].get(),
            Game::Scoring(after) => after.tricks_won[winner.index()].get(),
            _ => return Ok(false),
        };
        Ok(after_count == before_count + 1)
    } else {
        Ok(match transition.next {
            Game::Playing(after) => after.trick == TrickProgress::Lead(winner),
            Game::Scoring(_) => true,
            _ => false,
        })
    }
}

fn scoring_transition(before: Game, transition: &Transition) -> Result<bool, ModelError> {
    let Game::Scoring(state) = before else {
        return Ok(true);
    };
    if transition.action != ModelAction::Settle {
        return Ok(false);
    }
    let Some(events) = transition.round_scores else {
        return Ok(false);
    };
    for player in Player::ALL {
        let (outcome, points, payment) = formal::round_score(
            state.bids[player.index()],
            state.tricks_won[player.index()].get(),
            state.ledger.round,
        )?;
        let event = events[player.index()];
        if event.outcome as u8 != outcome
            || event.points != points
            || event.payment_cents != payment
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[allow(
    clippy::too_many_arguments,
    reason = "property declarations keep every catalog column explicit at each call site"
)]
fn spec(
    id: PropertyId,
    stable_id: &'static str,
    class: PropertyClass,
    expression: PropertyExpression,
    strategy: VerificationStrategy,
    explanation: &'static str,
    rules: &[(&'static str, &'static str)],
    checks: &[(&'static str, &'static str)],
) -> PropertySpec {
    PropertySpec {
        id,
        stable_id,
        class,
        expression,
        strategy,
        explanation,
        rules: rules
            .iter()
            .map(|(rule_id, source)| RuleOrigin { rule_id, source })
            .collect(),
        native_checks: checks
            .iter()
            .map(|(backend, check_id)| NativeCheckRef { backend, check_id })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{Bid, ChanceAction, Deal, RoundId};

    #[test]
    fn property_catalog_is_complete_unique_and_executable() {
        let catalog = property_catalog();
        assert_eq!(catalog.len(), 14);
        assert_eq!(
            catalog
                .iter()
                .map(|property| property.id)
                .collect::<BTreeSet<_>>()
                .len(),
            catalog.len()
        );
        assert_eq!(
            catalog
                .iter()
                .map(|property| property.stable_id)
                .collect::<BTreeSet<_>>()
                .len(),
            catalog.len()
        );
        assert!(catalog.iter().all(|property| {
            !property.explanation.is_empty()
                && !property.rules.is_empty()
                && !property.native_checks.is_empty()
        }));
        let kernels: BTreeSet<KernelId> = crate::formal_kernel_catalog()
            .unwrap()
            .into_iter()
            .map(|kernel| kernel.id)
            .collect();
        assert!(catalog.iter().all(|property| match property.expression {
            PropertyExpression::Kernel(kernel) => kernels.contains(&kernel),
            _ => true,
        }));
    }

    #[test]
    fn property_catalog_local_obligations_hold_on_a_complete_trace() {
        let mut game = Game::new(Player::One);
        let properties = property_catalog();
        for _ in 0..100 {
            for property in &properties {
                assert!(
                    evaluate_state_property(property.id, game).unwrap(),
                    "state property {:?}",
                    property.id
                );
            }
            if game.phase() == Phase::Finished {
                let transition = game.transition(ModelAction::Absorb).unwrap();
                for property in &properties {
                    assert!(
                        evaluate_transition_property(property.id, game, &transition).unwrap(),
                        "terminal transition property {:?}",
                        property.id
                    );
                }
                return;
            }
            let action = match game.legal_actions() {
                LegalActions::Chance(_) => ModelAction::Chance(ChanceAction::Deal(
                    Deal::all_for(match game {
                        Game::AwaitingDeal(state) => state.ledger.round,
                        _ => RoundId::OneAscending,
                    })[0],
                )),
                LegalActions::Player(actions) => ModelAction::Player(
                    actions
                        .into_iter()
                        .find(|action| !matches!(action, PlayerAction::Bid { bid: Bid::Two, .. }))
                        .expect("player has a non-two legal action"),
                ),
                LegalActions::Environment => ModelAction::Settle,
                LegalActions::Finished => unreachable!(),
            };
            let transition = game.transition(action).unwrap();
            for property in &properties {
                assert!(
                    evaluate_transition_property(property.id, game, &transition).unwrap(),
                    "transition property {:?}",
                    property.id
                );
            }
            game = transition.next;
        }
        panic!("complete micro-game trace did not terminate");
    }
}

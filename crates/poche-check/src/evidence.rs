use std::error::Error;
use std::fmt;

use phon::api;
use poche_interchange::{
    BackendKindWire, BackendWire, ConfidenceKindWire, ConfidenceWire, EnvironmentActionWire,
    EvidenceBundleWire, EvidenceContextWire, FixtureWire, ModelIdentityWire, ObservationWire,
    PhaseWire, PlayedCardWire, PlayerActionWire, ProjectionDiffWire, ProjectionKindWire,
    RawDiagnosticWire, RoundScoreWire, RuleRefWire, ScopeWire, SemanticHashWire, SolverResultWire,
    SolverStatusWire, StateDiffWire, StateWire, StatisticsWire, SubjectKindWire, SubjectWire,
    TraceStepWire, TraceWire, TransitionWire, ValidatedEvidence,
};
use poche_model::{
    Bid, CardSet, Game, LegalActions, ModelAction, Player, PlayerAction, RoundId, RoundOutcome,
    TrickProgress, TurnOwner,
};

use crate::{Edge, ExplicitGraph, StateId};

/// Controlled semantic mutation used to prove a safety check is discriminating.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InjectedDefect {
    /// A follower may ignore a lead suit that is present in hand.
    FollowSuitAllowsAnyCard,
    /// Raw rank decides a trick even when the lower card is trump.
    TrickWinnerUsesRankOnly,
    /// One viewer receives the opponent's private hand.
    ObservationLeaksOpponentHand,
    /// An all-tricks bid receives the partial exact-bid bonus.
    AllTricksUsesPartialScore,
}

impl InjectedDefect {
    /// Stable fixture/property ID.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::FollowSuitAllowsAnyCard => "injected-follow-suit-allows-any-card",
            Self::TrickWinnerUsesRankOnly => "injected-trick-winner-rank-only",
            Self::ObservationLeaksOpponentHand => "injected-observation-leak",
            Self::AllTricksUsesPartialScore => "injected-all-tricks-partial-score",
        }
    }
}

/// Validated common-wire counterexample plus its Phon encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefectEvidence {
    /// Injected fault detected.
    pub defect: InjectedDefect,
    /// Semantically validated common interchange envelope.
    pub bundle: EvidenceBundleWire,
    /// Phon bytes that decode to `bundle`.
    pub phon: Vec<u8>,
}

/// Failure to find, translate, validate, or encode an injected-defect witness.
#[derive(Debug)]
pub struct EvidenceError(String);

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for EvidenceError {}

#[derive(Clone, Debug, Default)]
struct RoundHistory {
    current: Vec<PlayedCardWire>,
    completed: Vec<Vec<PlayedCardWire>>,
}

#[derive(Clone, Debug)]
struct Witness {
    defect: InjectedDefect,
    state: StateId,
    edge: Option<Edge>,
    rules: &'static [(&'static str, &'static str)],
    diff: ProjectionDiffWire,
    summary: String,
}

/// Produce one discriminating, validated, Phon-encoded counterexample for each
/// required controlled defect.
///
/// # Errors
///
/// Returns an error if a discriminating reachable G4 state is absent or common
/// wire translation/validation/Phon roundtrip fails.
pub fn injected_defect_evidence(
    graph: &ExplicitGraph,
) -> Result<Vec<DefectEvidence>, EvidenceError> {
    [
        follow_suit_witness(graph)?,
        trick_winner_witness(graph)?,
        observation_witness(graph)?,
        scoring_witness(graph)?,
    ]
    .into_iter()
    .map(|witness| build_evidence(graph, witness))
    .collect()
}

fn follow_suit_witness(graph: &ExplicitGraph) -> Result<Witness, EvidenceError> {
    for (index, game) in graph.states().iter().copied().enumerate() {
        let Game::Playing(state) = game else {
            continue;
        };
        if state.ledger().round() != RoundId::Two {
            continue;
        }
        let TrickProgress::Follow(lead) = state.trick() else {
            continue;
        };
        let actor = state.actor();
        let hand = state.hand(actor);
        let Some(illegal) = hand.iter().find(|card| card.suit() != lead.card.suit()) else {
            continue;
        };
        if !hand.contains_suit(lead.card.suit()) {
            continue;
        }
        let legal = player_action_strings(game);
        let mut mutant = legal.clone();
        mutant.push(format!("play:{}:{}", actor.index(), illegal.index()));
        mutant.sort();
        return Ok(Witness {
            defect: InjectedDefect::FollowSuitAllowsAnyCard,
            state: state_id(index)?,
            edge: None,
            rules: &[
                ("R-TRICK-005", "docs/main.typ:292-294"),
                ("R-TRICK-006", "docs/main.typ:294"),
            ],
            diff: ProjectionDiffWire {
                projection: ProjectionKindWire::LegalActions,
                path: "legal_actions.play".to_owned(),
                expected: format!("{legal:?}"),
                actual: format!("{mutant:?}"),
                rule_ids: vec!["R-TRICK-005".to_owned(), "R-TRICK-006".to_owned()],
            },
            summary: format!(
                "mutant admits off-suit card {illegal:?} while actor {actor:?} still holds lead suit {}",
                lead.card.suit()
            ),
        });
    }
    Err(EvidenceError(
        "no reachable discriminating follow-suit state".to_owned(),
    ))
}

fn trick_winner_witness(graph: &ExplicitGraph) -> Result<Witness, EvidenceError> {
    for edge in graph.edges().iter().copied() {
        let Some(Game::Playing(state)) = graph.state(edge.from) else {
            continue;
        };
        if state.ledger().round() != RoundId::Two {
            continue;
        }
        let TrickProgress::Follow(lead) = state.trick() else {
            continue;
        };
        let ModelAction::Player(PlayerAction::Play { player, card }) = edge.action else {
            continue;
        };
        if card.suit() == state.trump().suit()
            && lead.card.suit() != state.trump().suit()
            && card.rank() < lead.card.rank()
        {
            return Ok(Witness {
                defect: InjectedDefect::TrickWinnerUsesRankOnly,
                state: edge.from,
                edge: Some(edge),
                rules: &[
                    ("R-TRICK-007", "docs/main.typ:296-302"),
                    ("R-TRICK-008", "docs/main.typ:296-302"),
                    ("R-TRICK-010", "docs/main.typ:301"),
                ],
                diff: ProjectionDiffWire {
                    projection: ProjectionKindWire::Transition,
                    path: "trick.winner".to_owned(),
                    expected: format!("player-{} (trump)", player.index()),
                    actual: format!("player-{} (raw higher rank)", lead.player.index()),
                    rule_ids: vec![
                        "R-TRICK-007".to_owned(),
                        "R-TRICK-008".to_owned(),
                        "R-TRICK-010".to_owned(),
                    ],
                },
                summary: format!(
                    "rank-only mutant selects lead {lead:?}; correct model selects lower-ranked trump {card:?}"
                ),
            });
        }
    }
    Err(EvidenceError(
        "no reachable lower-ranked-trump winner edge".to_owned(),
    ))
}

fn observation_witness(graph: &ExplicitGraph) -> Result<Witness, EvidenceError> {
    for (index, game) in graph.states().iter().copied().enumerate() {
        let Game::Bidding(state) = game else {
            continue;
        };
        if state.ledger().round() != RoundId::Two {
            continue;
        }
        let viewer = Player::Zero;
        let expected = state
            .hand(viewer)
            .iter()
            .map(poche_model::Card::index)
            .collect::<Vec<_>>();
        let mut leaked = expected.clone();
        leaked.extend(
            state
                .hand(viewer.left())
                .iter()
                .map(poche_model::Card::index),
        );
        leaked.sort_unstable();
        return Ok(Witness {
            defect: InjectedDefect::ObservationLeaksOpponentHand,
            state: state_id(index)?,
            edge: None,
            rules: &[
                ("R-GAME-004", "docs/main.typ:70-79"),
                ("R-BID-003", "docs/main.typ:278"),
                ("R-TRICK-004", "docs/main.typ:290"),
            ],
            diff: ProjectionDiffWire {
                projection: ProjectionKindWire::Observation,
                path: "viewer[0].own_hand".to_owned(),
                expected: format!("{expected:?}"),
                actual: format!("{leaked:?}"),
                rule_ids: vec![
                    "R-GAME-004".to_owned(),
                    "R-BID-003".to_owned(),
                    "R-TRICK-004".to_owned(),
                ],
            },
            summary: "mutant appends player one's private cards to player zero's observation"
                .to_owned(),
        });
    }
    Err(EvidenceError(
        "no reachable two-card bidding observation".to_owned(),
    ))
}

fn scoring_witness(graph: &ExplicitGraph) -> Result<Witness, EvidenceError> {
    for (index, game) in graph.states().iter().copied().enumerate() {
        let Game::Scoring(state) = game else {
            continue;
        };
        if state.ledger().round() != RoundId::Two {
            continue;
        }
        let Some(player) = Player::ALL.into_iter().find(|player| {
            state.bids()[player.index()] == Bid::Two
                && state.tricks_won()[player.index()].get() == 2
        }) else {
            continue;
        };
        let transition = game
            .transition(ModelAction::Settle)
            .map_err(|error| EvidenceError(format!("settlement witness failed: {error}")))?;
        let event = transition
            .round_scores
            .ok_or_else(|| EvidenceError("settlement omitted score events".to_owned()))?
            [player.index()];
        if event.outcome != RoundOutcome::AllTricks || event.points != 22 {
            continue;
        }
        return Ok(Witness {
            defect: InjectedDefect::AllTricksUsesPartialScore,
            state: state_id(index)?,
            edge: None,
            rules: &[
                ("R-SCORE-003", "docs/main.typ:317-323"),
                ("R-SCORE-004", "docs/main.typ:317-323"),
                ("R-SCORE-005", "docs/main.typ:323"),
            ],
            diff: ProjectionDiffWire {
                projection: ProjectionKindWire::RoundScore,
                path: format!("round_scores[{}].points", player.index()),
                expected: "22 (20 + bid)".to_owned(),
                actual: "12 (10 + bid)".to_owned(),
                rule_ids: vec![
                    "R-SCORE-003".to_owned(),
                    "R-SCORE-004".to_owned(),
                    "R-SCORE-005".to_owned(),
                ],
            },
            summary: format!(
                "mutant gives player {player:?} the partial exact bonus after winning all two tricks"
            ),
        });
    }
    Err(EvidenceError(
        "no reachable all-tricks scoring state".to_owned(),
    ))
}

fn build_evidence(
    graph: &ExplicitGraph,
    witness: Witness,
) -> Result<DefectEvidence, EvidenceError> {
    let context = context(&witness);
    let game = graph
        .state(witness.state)
        .ok_or_else(|| EvidenceError("witness state is absent".to_owned()))?;
    let history = history_at(graph, witness.state)?;
    let state = state_wire(game, u64::from(witness.state.get()), &history)?;
    let observations = Player::ALL
        .into_iter()
        .map(|viewer| observation_wire(game, u64::from(witness.state.get()), viewer))
        .collect::<Result<Vec<_>, _>>()?;
    let legal_actions = legal_player_actions(game);
    let transition = witness
        .edge
        .map(|edge| transition_wire(graph, edge, &history))
        .transpose()?;
    let trace = TraceWire {
        context: context.clone(),
        fixture_id: witness.defect.id().to_owned(),
        initial: state.clone(),
        steps: transition
            .clone()
            .into_iter()
            .map(|transition| TraceStepWire {
                index: 0,
                observation: acting_observation(game, u64::from(witness.state.get())),
                legal_actions: legal_actions.clone(),
                transition,
            })
            .collect(),
        cycle_start: None,
    };
    let measurements = graph.stats();
    let bundle = EvidenceBundleWire {
        fixture: FixtureWire {
            context: context.clone(),
            fixture_id: witness.defect.id().to_owned(),
            description: witness.summary.clone(),
            state,
            observations,
            legal_actions,
            chance_actions: Vec::new(),
            expected_transition: transition,
        },
        result: SolverResultWire {
            context,
            status: SolverStatusWire::Counterexample,
            summary: witness.summary,
            bindings: Vec::new(),
            trace: Some(trace),
            statistics: StatisticsWire {
                states: Some(to_u64(measurements.states)?),
                transitions: Some(to_u64(measurements.transitions)?),
                max_depth: Some(u64::from(measurements.maximum_depth)),
                duplicates: Some(to_u64(measurements.duplicate_state_hits)?),
                duration_ms: 0,
                backend: Vec::new(),
            },
            raw_diagnostics: vec![RawDiagnosticWire {
                stream: "injected-defect".to_owned(),
                severity: "error".to_owned(),
                text: format!("controlled defect {:?} is discriminated", witness.defect),
            }],
            counterexample_diffs: vec![witness.diff],
        },
    };
    ValidatedEvidence::try_from(bundle.clone())
        .map_err(|error| EvidenceError(format!("wire validation failed: {error}")))?;
    let phon = api::encode(&bundle)
        .map_err(|error| EvidenceError(format!("Phon encode failed: {error}")))?;
    let decoded: EvidenceBundleWire = api::decode(&phon)
        .map_err(|error| EvidenceError(format!("Phon decode failed: {error}")))?;
    if decoded != bundle {
        return Err(EvidenceError("Phon roundtrip changed evidence".to_owned()));
    }
    Ok(DefectEvidence {
        defect: witness.defect,
        bundle,
        phon,
    })
}

fn context(witness: &Witness) -> EvidenceContextWire {
    EvidenceContextWire {
        model: ModelIdentityWire {
            model_id: "poche-rust-formal".to_owned(),
            model_revision: env!("CARGO_PKG_VERSION").to_owned(),
            schema_id: "poche.interchange.evidence.v1".to_owned(),
            schema_semantic_hash: SemanticHashWire([1; 32]),
            rules_revision: "rules-2026-08-03".to_owned(),
            rules_semantic_hash: SemanticHashWire([2; 32]),
            observation_semantic_hash: SemanticHashWire([3; 32]),
            scoring_semantic_hash: SemanticHashWire([4; 32]),
        },
        scope: ScopeWire {
            scope_id: "micro-2p-2s-3r-2h".to_owned(),
            player_count: 2,
            suit_count: 2,
            ranks_per_suit: 3,
            deck_size: 6,
            cards_per_player: 2,
            trump_card_count: 1,
            undealt_card_count: 1,
            exhaustive: true,
        },
        backend: BackendWire {
            kind: BackendKindWire::RustExplicit,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        subject: SubjectWire {
            kind: SubjectKindWire::Property,
            id: witness.defect.id().to_owned(),
        },
        rules: witness
            .rules
            .iter()
            .map(|(rule_id, source)| RuleRefWire {
                rule_id: (*rule_id).to_owned(),
                source: (*source).to_owned(),
            })
            .collect(),
        confidence: ConfidenceWire {
            kind: ConfidenceKindWire::Exhaustive,
            qualification: "discriminating witness found in complete reachable micro-game graph"
                .to_owned(),
        },
    }
}

fn history_at(graph: &ExplicitGraph, state: StateId) -> Result<RoundHistory, EvidenceError> {
    let trace = graph
        .shortest_trace(state)
        .ok_or_else(|| EvidenceError("cannot reconstruct witness history".to_owned()))?;
    let mut history = RoundHistory::default();
    for action in trace.actions {
        update_history(&mut history, action);
    }
    Ok(history)
}

fn update_history(history: &mut RoundHistory, action: ModelAction) {
    match action {
        ModelAction::Chance(_) | ModelAction::Settle => {
            history.current.clear();
            history.completed.clear();
        }
        ModelAction::Player(PlayerAction::Play { player, card }) => {
            history.current.push(PlayedCardWire {
                player: player_index(player),
                card: card_index(card),
            });
            if history.current.len() == 2 {
                history.completed.push(std::mem::take(&mut history.current));
            }
        }
        ModelAction::Player(PlayerAction::Bid { .. }) | ModelAction::Absorb => {}
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "phase-specific extraction is deliberately exhaustive at the wire boundary"
)]
fn state_wire(
    game: Game,
    state_id: u64,
    history: &RoundHistory,
) -> Result<StateWire, EvidenceError> {
    let (phase, ledger, actor, hands, trump, undealt, bids, tricks) = match game {
        Game::AwaitingDeal(state) => (
            PhaseWire::AwaitingDeal,
            state.ledger(),
            None,
            [CardSet::EMPTY; 2],
            None,
            CardSet::EMPTY,
            [None, None],
            [0, 0],
        ),
        Game::Bidding(state) => (
            PhaseWire::Bidding,
            state.ledger(),
            Some(player_index(state.actor())),
            [state.hand(Player::Zero), state.hand(Player::One)],
            Some(state.trump()),
            state.undealt(),
            state.bids().map(|bid| bid.map(Bid::get)),
            [0, 0],
        ),
        Game::Playing(state) => (
            PhaseWire::Playing,
            state.ledger(),
            Some(player_index(state.actor())),
            [state.hand(Player::Zero), state.hand(Player::One)],
            Some(state.trump()),
            state.undealt(),
            state.bids().map(|bid| Some(bid.get())),
            state.tricks_won().map(poche_model::Tricks::get),
        ),
        Game::Scoring(state) => (
            PhaseWire::Scoring,
            state.ledger(),
            None,
            [CardSet::EMPTY; 2],
            Some(state.trump()),
            state.undealt(),
            state.bids().map(|bid| Some(bid.get())),
            state.tricks_won().map(poche_model::Tricks::get),
        ),
        Game::Finished(_) => {
            return Err(EvidenceError(
                "G4 defect evidence cannot encode a schedule-terminal state".to_owned(),
            ));
        }
    };
    if ledger.round() != RoundId::Two {
        return Err(EvidenceError(format!(
            "G4 evidence requires RoundId::Two, found {:?}",
            ledger.round()
        )));
    }
    Ok(StateWire {
        state_id,
        phase,
        dealer: player_index(ledger.dealer()),
        actor,
        hand_size: 2,
        hands: hands.map(card_set).to_vec(),
        trump: trump.map(card_index),
        undealt: card_set(undealt),
        current_trick: history.current.clone(),
        completed_tricks: history.completed.clone(),
        bids: bids.to_vec(),
        tricks_won: tricks.to_vec(),
        cumulative_scores: ledger.scores().map(|score| i32::from(score.get())).to_vec(),
        pot_cents: u32::from(ledger.pot().cents()),
    })
}

fn observation_wire(
    game: Game,
    state_id: u64,
    viewer: Player,
) -> Result<ObservationWire, EvidenceError> {
    let observation = game.observe(viewer);
    let dealer = observation
        .dealer
        .ok_or_else(|| EvidenceError("G4 observation has no dealer".to_owned()))?;
    Ok(ObservationWire {
        state_id,
        viewer: player_index(viewer),
        phase: phase_wire(observation.phase),
        dealer: player_index(dealer),
        actor: match observation.actor {
            TurnOwner::Player(player) => Some(player_index(player)),
            TurnOwner::Chance | TurnOwner::Environment | TurnOwner::Finished => None,
        },
        own_hand: card_set(observation.private_hand),
        hand_counts: observation.hand_counts.to_vec(),
        trump: observation.trump.map(card_index),
        current_trick: observation
            .current_trick
            .into_iter()
            .map(|play| PlayedCardWire {
                player: player_index(play.player),
                card: card_index(play.card),
            })
            .collect(),
        bids: observation.bids.map(|bid| bid.map(Bid::get)).to_vec(),
        tricks_won: observation
            .tricks_won
            .map(poche_model::Tricks::get)
            .to_vec(),
        cumulative_scores: observation
            .scores
            .map(|score| i32::from(score.get()))
            .to_vec(),
        pot_cents: u32::from(observation.pot.cents()),
    })
}

fn acting_observation(game: Game, state_id: u64) -> Option<ObservationWire> {
    let TurnOwner::Player(player) = game.turn() else {
        return None;
    };
    observation_wire(game, state_id, player).ok()
}

fn legal_player_actions(game: Game) -> Vec<PlayerActionWire> {
    let LegalActions::Player(actions) = game.legal_actions() else {
        return Vec::new();
    };
    actions.into_iter().map(player_action_wire).collect()
}

fn player_action_wire(action: PlayerAction) -> PlayerActionWire {
    match action {
        PlayerAction::Bid { player, bid } => PlayerActionWire::Bid {
            player: player_index(player),
            tricks: bid.get(),
        },
        PlayerAction::Play { player, card } => PlayerActionWire::Play {
            player: player_index(player),
            card: card_index(card),
        },
    }
}

fn transition_wire(
    graph: &ExplicitGraph,
    edge: Edge,
    before_history: &RoundHistory,
) -> Result<TransitionWire, EvidenceError> {
    let before = graph
        .state(edge.from)
        .ok_or_else(|| EvidenceError("edge source is absent".to_owned()))?;
    let transition = before
        .transition(edge.action)
        .map_err(|error| EvidenceError(format!("edge replay failed: {error}")))?;
    let mut history = before_history.clone();
    update_history(&mut history, edge.action);
    let after = state_wire(transition.next, u64::from(edge.to.get()), &history)?;
    let action = match edge.action {
        ModelAction::Player(action) => EnvironmentActionWire::Player(player_action_wire(action)),
        ModelAction::Settle => EnvironmentActionWire::Settle,
        ModelAction::Chance(_) | ModelAction::Absorb => {
            return Err(EvidenceError(
                "defect transition witness must be player/settlement owned".to_owned(),
            ));
        }
    };
    Ok(TransitionWire {
        before_state_id: u64::from(edge.from.get()),
        action,
        after,
        round_scores: transition
            .round_scores
            .into_iter()
            .flatten()
            .map(|event| RoundScoreWire {
                player: player_index(event.player),
                bid: event.bid.get(),
                tricks_won: event.tricks_won,
                points: u16::from(event.points),
                payment_cents: u32::from(event.payment_cents),
                score_rule_id: match event.outcome {
                    RoundOutcome::Miss => "R-SCORE-002",
                    RoundOutcome::Exact => "R-SCORE-003",
                    RoundOutcome::AllTricks => "R-SCORE-004",
                }
                .to_owned(),
                money_rule_id: if event.payment_cents == 0 {
                    "R-MONEY-002"
                } else {
                    "R-MONEY-001"
                }
                .to_owned(),
            })
            .collect(),
        game_outcome: None,
        diffs: transition
            .diffs
            .into_iter()
            .map(|diff| StateDiffWire {
                path: diff.path.to_owned(),
                before: Some(diff.before),
                after: Some(diff.after),
                rule_ids: diff
                    .origins
                    .into_iter()
                    .map(|origin| origin.rule_id.to_owned())
                    .collect(),
            })
            .collect(),
        rule_ids: transition
            .origins
            .into_iter()
            .map(|origin| origin.rule_id.to_owned())
            .collect(),
    })
}

fn phase_wire(phase: poche_model::Phase) -> PhaseWire {
    match phase {
        poche_model::Phase::AwaitingDeal => PhaseWire::AwaitingDeal,
        poche_model::Phase::Bidding => PhaseWire::Bidding,
        poche_model::Phase::Playing => PhaseWire::Playing,
        poche_model::Phase::Scoring => PhaseWire::Scoring,
        poche_model::Phase::Finished => PhaseWire::Finished,
    }
}

fn player_action_strings(game: Game) -> Vec<String> {
    let mut actions = legal_player_actions(game)
        .into_iter()
        .map(|action| match action {
            PlayerActionWire::Bid { player, tricks } => format!("bid:{player}:{tricks}"),
            PlayerActionWire::Play { player, card } => format!("play:{player}:{card}"),
        })
        .collect::<Vec<_>>();
    actions.sort();
    actions
}

fn card_set(cards: CardSet) -> Vec<u8> {
    cards.iter().map(card_index).collect()
}

fn player_index(player: Player) -> u8 {
    u8::try_from(player.index()).expect("two-player index fits u8")
}

fn card_index(card: poche_model::Card) -> u8 {
    u8::try_from(card.index()).expect("six-card index fits u8")
}

fn state_id(index: usize) -> Result<StateId, EvidenceError> {
    Ok(StateId(u32::try_from(index).map_err(|_| {
        EvidenceError("state index exceeds u32".to_owned())
    })?))
}

fn to_u64(value: usize) -> Result<u64, EvidenceError> {
    u64::try_from(value).map_err(|_| EvidenceError("statistic exceeds u64".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exhaustive_test_graph;

    #[test]
    fn injected_defects_have_valid_relevant_phon_counterexamples() {
        let evidence = injected_defect_evidence(exhaustive_test_graph()).unwrap();
        assert_eq!(evidence.len(), 4);
        assert_eq!(
            evidence.iter().map(|item| item.defect).collect::<Vec<_>>(),
            vec![
                InjectedDefect::FollowSuitAllowsAnyCard,
                InjectedDefect::TrickWinnerUsesRankOnly,
                InjectedDefect::ObservationLeaksOpponentHand,
                InjectedDefect::AllTricksUsesPartialScore,
            ]
        );
        for item in evidence {
            assert!(!item.phon.is_empty());
            assert_eq!(item.bundle.result.status, SolverStatusWire::Counterexample);
            assert_eq!(item.bundle.result.counterexample_diffs.len(), 1);
            assert!(
                !item.bundle.result.counterexample_diffs[0]
                    .rule_ids
                    .is_empty()
            );
            let decoded: EvidenceBundleWire = api::decode(&item.phon).unwrap();
            assert_eq!(decoded, item.bundle);
            ValidatedEvidence::try_from(decoded).unwrap();
        }
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    AlloyCommandResult, NativeBackend, NativeDisposition, NormalizedRun, NuSmvPropertyKind,
};

/// Inventory category of a common fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FixtureKind {
    /// Concrete scenario.
    Scenario,
    /// Safety or liveness property.
    Property,
    /// Relational query surface.
    Query,
}

/// Native-language selector used to evaluate a shared fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeSelector {
    /// Stable Alloy `run` or `check` command name.
    AlloyCommand(&'static str),
    /// Stable `NuSMV` output expression and category.
    NuSmvProperty {
        /// Specification or invariant.
        kind: NuSmvPropertyKind,
        /// Expression as normalized by `NuSMV` 2.7.1.
        expression: &'static str,
    },
    /// Stable `oracle_test/1` name in the Scryer corpus.
    PrologTest(&'static str),
}

/// Explicit conversion from a shared inventory item to native conventions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureAdapter {
    /// ID from `fixtures/oracle-inventory.toml`.
    pub fixture_id: &'static str,
    /// Inventory category.
    pub kind: FixtureKind,
    /// Native target.
    pub backend: NativeBackend,
    /// Native checks whose conjunction establishes this fixture's evidence.
    pub selectors: Vec<NativeSelector>,
    /// Constraint/mode note that prevents overclaiming API equivalence.
    pub convention: &'static str,
}

/// Result for one selector within an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorEvaluation {
    /// Selector that was requested.
    pub selector: NativeSelector,
    /// `Some(true/false)` for recognized output; `None` for absent output.
    pub passed: Option<bool>,
}

/// Common normalized answer for one fixture/backend pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureEvaluation {
    /// Shared fixture ID.
    pub fixture_id: &'static str,
    /// Native backend.
    pub backend: NativeBackend,
    /// Success, recognized failure, or unknown/absent selector.
    pub disposition: NativeDisposition,
    /// Per-selector native evidence.
    pub selectors: Vec<SelectorEvaluation>,
    /// Adapter constraint/mode note.
    pub convention: &'static str,
}

/// Evaluate every adapter for one backend against one normalized native run.
#[must_use]
pub fn evaluate_fixtures(
    backend: NativeBackend,
    normalized: &NormalizedRun,
) -> Vec<FixtureEvaluation> {
    fixture_adapters()
        .into_iter()
        .filter(|adapter| adapter.backend == backend)
        .map(|adapter| {
            let selectors: Vec<_> = adapter
                .selectors
                .iter()
                .cloned()
                .map(|selector| SelectorEvaluation {
                    passed: selector_result(&selector, normalized),
                    selector,
                })
                .collect();
            let disposition = if selectors.iter().any(|result| result.passed == Some(false)) {
                NativeDisposition::Failure
            } else if selectors.iter().any(|result| result.passed.is_none()) {
                NativeDisposition::Unknown
            } else {
                NativeDisposition::Success
            };
            FixtureEvaluation {
                fixture_id: adapter.fixture_id,
                backend,
                disposition,
                selectors,
                convention: adapter.convention,
            }
        })
        .collect()
}

fn selector_result(selector: &NativeSelector, normalized: &NormalizedRun) -> Option<bool> {
    match (selector, normalized) {
        (NativeSelector::AlloyCommand(name), NormalizedRun::Alloy(results)) => results
            .iter()
            .find(|result| result.name == *name)
            .map(AlloyCommandResult::passed),
        (NativeSelector::NuSmvProperty { kind, expression }, NormalizedRun::NuSmv(results)) => {
            results
                .iter()
                .find(|result| result.kind == *kind && result.expression == *expression)
                .map(|result| result.holds)
        }
        (NativeSelector::PrologTest(name), NormalizedRun::ScryerProlog(results)) => results
            .iter()
            .find(|result| result.name == *name)
            .map(|result| result.passed),
        _ => None,
    }
}

/// Complete, auditable fixture-to-native selector registry.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn fixture_adapters() -> Vec<FixtureAdapter> {
    use FixtureKind::{Property, Query, Scenario};
    use NativeBackend::{Alloy, NuSmv, ScryerProlog};
    use NativeSelector::{AlloyCommand, NuSmvProperty, PrologTest};
    use NuSmvPropertyKind::{Invariant, Specification};

    const ALLOY_SCOPE: &str = "Bounded command; exact scope is retained from Alloy receipt source.";
    const NUSMV_SCOPE: &str =
        "Exhaustive only for the handwritten two-player symbolic transition abstraction.";
    const PROLOG_SCOPE: &str =
        "Finite productive query mode from the named handwritten Prolog corpus.";

    let mut adapters = Vec::new();
    let mut add = |fixture_id, kind, backend, selectors, convention| {
        adapters.push(FixtureAdapter {
            fixture_id,
            kind,
            backend,
            selectors,
            convention,
        });
    };

    add(
        "schedule-boundaries",
        Scenario,
        Alloy,
        vec![
            AlloyCommand("ParameterBoundaryWitness"),
            AlloyCommand("ScheduleBoundariesAndFeasibility"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "schedule-boundaries",
        Scenario,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Invariant,
                expression: "(max_hand_for_table >= 1 & max_hand_for_table <= 7)",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "table_players * max_hand_for_table + 1 <= 52",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "rounds_for_table = 2 * max_hand_for_table - 1",
            },
        ],
        NUSMV_SCOPE,
    );
    add(
        "schedule-boundaries",
        Scenario,
        ScryerProlog,
        vec![PrologTest("all_player_schedules")],
        PROLOG_SCOPE,
    );

    add(
        "complete-two-player-round",
        Scenario,
        Alloy,
        vec![
            AlloyCommand("CompleteRoundWitness"),
            AlloyCommand("CardConservationAndPartition"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "complete-two-player-round",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF phase = finished",
        }],
        NUSMV_SCOPE,
    );
    add(
        "complete-two-player-round",
        Scenario,
        ScryerProlog,
        vec![PrologTest("forward_round_trace")],
        PROLOG_SCOPE,
    );

    add(
        "unrestricted-total-bid",
        Scenario,
        Alloy,
        vec![AlloyCommand("UnrestrictedBidWitness")],
        ALLOY_SCOPE,
    );
    add(
        "unrestricted-total-bid",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF (phase = play_lead & bid0 + bid1 != hand_size)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "unrestricted-total-bid",
        Scenario,
        ScryerProlog,
        vec![PrologTest("bid_domain")],
        PROLOG_SCOPE,
    );

    add(
        "zero-bid-success",
        Scenario,
        Alloy,
        vec![
            AlloyCommand("ZeroBidSuccessWitness"),
            AlloyCommand("ScoreAndPaymentAgree"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "zero-bid-success",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF (((phase = advance & bid0 = 0) & tricks0 = 0) & round_score0 = 10)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "zero-bid-success",
        Scenario,
        ScryerProlog,
        vec![PrologTest("scoring_and_reverse_scoring")],
        PROLOG_SCOPE,
    );

    add(
        "follow-suit-required",
        Scenario,
        Alloy,
        vec![AlloyCommand("FollowSuitIsEnforced")],
        ALLOY_SCOPE,
    );
    add(
        "follow-suit-required",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Invariant,
            expression: "((phase = collect & follow_had_lead) -> follow_suit = lead_suit)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "follow-suit-required",
        Scenario,
        ScryerProlog,
        vec![PrologTest("follow_suit")],
        PROLOG_SCOPE,
    );

    add(
        "void-player-may-trump",
        Scenario,
        Alloy,
        vec![
            AlloyCommand("CompleteRoundWitness"),
            AlloyCommand("WinnerIsEligibleAndHighest"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "void-player-may-trump",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF (((phase = collect & lead_suit != trump_suit) & follow_suit = trump_suit) & last_winner = follower_player)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "void-player-may-trump",
        Scenario,
        ScryerProlog,
        vec![PrologTest("void_play"), PrologTest("trump_winner")],
        PROLOG_SCOPE,
    );

    add(
        "off-suit-cannot-win",
        Scenario,
        Alloy,
        vec![AlloyCommand("WinnerIsEligibleAndHighest")],
        ALLOY_SCOPE,
    );
    add(
        "off-suit-cannot-win",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Invariant,
            expression: "(phase = collect -> last_winner = expected_winner)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "off-suit-cannot-win",
        Scenario,
        ScryerProlog,
        vec![PrologTest("lead_winner")],
        PROLOG_SCOPE,
    );

    for fixture_id in ["exact-partial-score", "all-tricks-score"] {
        add(
            fixture_id,
            Scenario,
            Alloy,
            vec![AlloyCommand("ScoreAndPaymentAgree")],
            ALLOY_SCOPE,
        );
        add(
            fixture_id,
            Scenario,
            NuSmv,
            vec![
                NuSmvProperty {
                    kind: Invariant,
                    expression: "(phase = advance -> round_score0 = expected_round_score0)",
                },
                NuSmvProperty {
                    kind: Invariant,
                    expression: "(phase = advance -> round_score1 = expected_round_score1)",
                },
            ],
            NUSMV_SCOPE,
        );
        add(
            fixture_id,
            Scenario,
            ScryerProlog,
            vec![PrologTest("scoring_and_reverse_scoring")],
            PROLOG_SCOPE,
        );
    }

    add(
        "missed-bid-payment",
        Scenario,
        Alloy,
        vec![AlloyCommand("ScoreAndPaymentAgree")],
        ALLOY_SCOPE,
    );
    add(
        "missed-bid-payment",
        Scenario,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Invariant,
                expression: "(phase = advance -> (paid_miss0 <-> tricks0 != bid0))",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "(phase = advance -> (paid_miss1 <-> tricks1 != bid1))",
            },
        ],
        NUSMV_SCOPE,
    );
    add(
        "missed-bid-payment",
        Scenario,
        ScryerProlog,
        vec![
            PrologTest("scoring_and_reverse_scoring"),
            PrologTest("money_and_shared_winners"),
        ],
        PROLOG_SCOPE,
    );

    add(
        "shared-final-winner",
        Scenario,
        Alloy,
        vec![
            AlloyCommand("SharedWinnerWitness"),
            AlloyCommand("FinalWinnersAreExactlyTheMaxima"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "shared-final-winner",
        Scenario,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Invariant,
                expression: "(phase = finished -> bowl_recipient0 = winner0)",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "(phase = finished -> bowl_recipient1 = winner1)",
            },
        ],
        NUSMV_SCOPE,
    );
    add(
        "shared-final-winner",
        Scenario,
        ScryerProlog,
        vec![PrologTest("money_and_shared_winners")],
        PROLOG_SCOPE,
    );

    add(
        "first-jack-seat-order",
        Scenario,
        Alloy,
        vec![AlloyCommand("FirstJackWitness")],
        ALLOY_SCOPE,
    );
    add(
        "first-jack-seat-order",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF selection_method = first_jack",
        }],
        NUSMV_SCOPE,
    );
    add(
        "first-jack-seat-order",
        Scenario,
        ScryerProlog,
        vec![PrologTest("first_jack_selection")],
        PROLOG_SCOPE,
    );

    add(
        "high-card-repeated-tie",
        Scenario,
        Alloy,
        vec![AlloyCommand("RepeatedHighCardWitness")],
        ALLOY_SCOPE,
    );
    add(
        "high-card-repeated-tie",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "EF selection_method = high_card",
        }],
        NUSMV_SCOPE,
    );
    add(
        "high-card-repeated-tie",
        Scenario,
        ScryerProlog,
        vec![PrologTest("repeated_high_card")],
        PROLOG_SCOPE,
    );

    add(
        "dealer-rotation",
        Scenario,
        Alloy,
        vec![AlloyCommand("ScheduleBoundariesAndFeasibility")],
        ALLOY_SCOPE,
    );
    add(
        "dealer-rotation",
        Scenario,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Specification,
                expression: "AG (((phase = advance & round_number < 13) & dealer = p0) -> AX dealer = p1)",
            },
            NuSmvProperty {
                kind: Specification,
                expression: "AG (((phase = advance & round_number < 13) & dealer = p1) -> AX dealer = p0)",
            },
        ],
        NUSMV_SCOPE,
    );
    add(
        "dealer-rotation",
        Scenario,
        ScryerProlog,
        vec![PrologTest("score_sheet_rotation")],
        PROLOG_SCOPE,
    );

    add(
        "final-one-card-round",
        Scenario,
        Alloy,
        vec![AlloyCommand("ScheduleBoundariesAndFeasibility")],
        ALLOY_SCOPE,
    );
    add(
        "final-one-card-round",
        Scenario,
        NuSmv,
        vec![NuSmvProperty {
            kind: Invariant,
            expression: "(phase = finished -> (round_number = 13 & hand_size = 1))",
        }],
        NUSMV_SCOPE,
    );
    add(
        "final-one-card-round",
        Scenario,
        ScryerProlog,
        vec![PrologTest("all_player_schedules")],
        PROLOG_SCOPE,
    );

    add(
        "full-deck-conservation",
        Property,
        Alloy,
        vec![
            AlloyCommand("CompleteDeckIsExactly52"),
            AlloyCommand("CardConservationAndPartition"),
        ],
        ALLOY_SCOPE,
    );
    add(
        "full-deck-conservation",
        Property,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Invariant,
                expression: "((deck_size = 52 & suit_count = 4) & rank_count = 13)",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "card_total = 52",
            },
            NuSmvProperty {
                kind: Invariant,
                expression: "((phase = advance | phase = finished) -> card_total = undealt_count)",
            },
        ],
        NUSMV_SCOPE,
    );
    add(
        "full-deck-conservation",
        Property,
        ScryerProlog,
        vec![
            PrologTest("standard_deck"),
            PrologTest("forward_round_trace"),
        ],
        PROLOG_SCOPE,
    );

    add(
        "no-reachable-deadlock",
        Property,
        NuSmv,
        vec![NuSmvProperty {
            kind: Specification,
            expression: "AG (EX TRUE)",
        }],
        NUSMV_SCOPE,
    );
    add(
        "universal-termination",
        Property,
        NuSmv,
        vec![
            NuSmvProperty {
                kind: Specification,
                expression: "AF phase = finished",
            },
            NuSmvProperty {
                kind: Specification,
                expression: "F phase = finished",
            },
        ],
        NUSMV_SCOPE,
    );

    add(
        "legal-actions-from-state",
        Query,
        ScryerProlog,
        vec![
            PrologTest("bid_domain"),
            PrologTest("follow_suit"),
            PrologTest("void_play"),
            PrologTest("forward_round_trace"),
        ],
        "Ground finite state; answers are normalized as sets in Phase 6.2.",
    );
    add(
        "predecessors-for-action-and-state",
        Query,
        ScryerProlog,
        vec![
            PrologTest("reverse_bid_predecessor"),
            PrologTest("reverse_play_predecessor"),
        ],
        "Ground bounded successor and action constrain productive reverse search.",
    );
    add(
        "score-causes",
        Query,
        ScryerProlog,
        vec![PrologTest("scoring_and_reverse_scoring")],
        "Finite hand size and score constrain reverse score explanations.",
    );

    adapters
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const SCENARIOS: [&str; 15] = [
        "schedule-boundaries",
        "complete-two-player-round",
        "unrestricted-total-bid",
        "zero-bid-success",
        "follow-suit-required",
        "void-player-may-trump",
        "off-suit-cannot-win",
        "exact-partial-score",
        "all-tricks-score",
        "missed-bid-payment",
        "shared-final-winner",
        "first-jack-seat-order",
        "high-card-repeated-tie",
        "dealer-rotation",
        "final-one-card-round",
    ];

    #[test]
    fn adapters_cover_every_native_inventory_track_exactly_once() {
        let adapters = fixture_adapters();
        let mut pairs = BTreeSet::new();
        for adapter in &adapters {
            assert!(!adapter.selectors.is_empty(), "{}", adapter.fixture_id);
            assert!(
                pairs.insert((adapter.fixture_id, adapter.backend)),
                "duplicate adapter for {} / {}",
                adapter.fixture_id,
                adapter.backend.id()
            );
        }
        for scenario in SCENARIOS {
            for backend in [
                NativeBackend::Alloy,
                NativeBackend::NuSmv,
                NativeBackend::ScryerProlog,
            ] {
                assert!(
                    pairs.contains(&(scenario, backend)),
                    "{scenario}/{}",
                    backend.id()
                );
            }
        }
        for backend in [
            NativeBackend::Alloy,
            NativeBackend::NuSmv,
            NativeBackend::ScryerProlog,
        ] {
            assert!(pairs.contains(&("full-deck-conservation", backend)));
        }
        assert!(pairs.contains(&("no-reachable-deadlock", NativeBackend::NuSmv)));
        assert!(pairs.contains(&("universal-termination", NativeBackend::NuSmv)));
        for query in [
            "legal-actions-from-state",
            "predecessors-for-action-and-state",
            "score-causes",
        ] {
            assert!(pairs.contains(&(query, NativeBackend::ScryerProlog)));
        }
        assert_eq!(pairs.len(), 53);
    }

    #[test]
    fn named_alloy_and_prolog_selectors_exist_in_handwritten_sources() {
        let alloy = include_str!("../../../models/alloy/poche.als");
        let prolog = include_str!("../../../models/prolog/poche.pl");
        for adapter in fixture_adapters() {
            for selector in adapter.selectors {
                match selector {
                    NativeSelector::AlloyCommand(name) => assert!(
                        alloy.contains(name),
                        "Alloy selector {name} is absent from source"
                    ),
                    NativeSelector::PrologTest(name) => assert!(
                        prolog.contains(&format!("oracle_test({name})")),
                        "Prolog selector {name} is absent from source"
                    ),
                    NativeSelector::NuSmvProperty { .. } => {}
                }
            }
        }
    }

    #[test]
    fn absent_selector_is_unknown_not_success() {
        let empty = NormalizedRun::Alloy(Vec::new());
        let results = evaluate_fixtures(NativeBackend::Alloy, &empty);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .all(|result| result.disposition == NativeDisposition::Unknown)
        );
    }
}

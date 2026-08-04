use poche_interchange::{
    BackendKindWire, BackendWire, ChanceActionWire, ChanceProvenanceWire, ConfidenceKindWire,
    ConfidenceWire, EnvironmentActionWire, EvidenceBundleWire, EvidenceContextWire, FixtureWire,
    GameOutcomeWire, ModelIdentityWire, ObservationWire, PhaseWire, PlayerActionWire, PotShareWire,
    ProjectionDiffWire, ProjectionKindWire, PrologBindingWire, RawDiagnosticWire, RoundScoreWire,
    RuleRefWire, ScopeWire, SemanticHashWire, SolverResultWire, SolverStatusWire, StateDiffWire,
    StateWire, StatisticWire, StatisticsWire, SubjectKindWire, SubjectWire, TraceStepWire,
    TraceWire, TransitionWire, ValidatedEvidence,
};

fn hash(byte: u8) -> SemanticHashWire {
    SemanticHashWire([byte; 32])
}

fn model(model_id: &str, revision: &str) -> ModelIdentityWire {
    ModelIdentityWire {
        model_id: model_id.to_owned(),
        model_revision: revision.to_owned(),
        schema_id: "poche.interchange.evidence.v1".to_owned(),
        schema_semantic_hash: hash(1),
        rules_revision: "rules-2026-08-03".to_owned(),
        rules_semantic_hash: hash(2),
        observation_semantic_hash: hash(3),
        scoring_semantic_hash: hash(4),
    }
}

fn scope() -> ScopeWire {
    ScopeWire {
        scope_id: "micro-2p-2s-3r-2h".to_owned(),
        player_count: 2,
        suit_count: 2,
        ranks_per_suit: 3,
        deck_size: 6,
        cards_per_player: 2,
        trump_card_count: 1,
        undealt_card_count: 1,
        exhaustive: true,
    }
}

fn context(
    model_id: &str,
    backend: BackendKindWire,
    confidence: ConfidenceKindWire,
) -> EvidenceContextWire {
    EvidenceContextWire {
        model: model(model_id, "test-revision"),
        scope: scope(),
        backend: BackendWire {
            kind: backend,
            version: "test-version".to_owned(),
        },
        subject: SubjectWire {
            kind: SubjectKindWire::Scenario,
            id: "unrestricted-total-bid".to_owned(),
        },
        rules: vec![RuleRefWire {
            rule_id: "R-BID-004".to_owned(),
            source: "docs/main.typ:bid".to_owned(),
        }],
        confidence: ConfidenceWire {
            kind: confidence,
            qualification: "exact named micro-scope".to_owned(),
        },
    }
}

fn state(state_id: u64, actor: u8, first_bid: Option<u8>) -> StateWire {
    StateWire {
        state_id,
        phase: PhaseWire::Bidding,
        dealer: 1,
        actor: Some(actor),
        hand_size: 2,
        hands: vec![vec![0, 1], vec![2, 3]],
        trump: Some(4),
        undealt: vec![5],
        current_trick: Vec::new(),
        completed_tricks: Vec::new(),
        bids: vec![first_bid, None],
        tricks_won: vec![0, 0],
        cumulative_scores: vec![0, 0],
        pot_cents: 0,
    }
}

fn observation(state: &StateWire, viewer: u8) -> ObservationWire {
    ObservationWire {
        state_id: state.state_id,
        viewer,
        phase: state.phase,
        dealer: state.dealer,
        actor: state.actor,
        own_hand: state.hands[usize::from(viewer)].clone(),
        hand_counts: vec![2, 2],
        trump: state.trump,
        current_trick: state.current_trick.clone(),
        bids: state.bids.clone(),
        tricks_won: state.tricks_won.clone(),
        cumulative_scores: state.cumulative_scores.clone(),
        pot_cents: state.pot_cents,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one literal bundle keeps every interchange field visible in the roundtrip fixture"
)]
fn sample_bundle() -> EvidenceBundleWire {
    let fixture_context = context(
        "poche-rust-oracle",
        BackendKindWire::RustOracle,
        ConfidenceKindWire::Simulated,
    );
    let result_context = context(
        "poche-alloy-oracle",
        BackendKindWire::Alloy,
        ConfidenceKindWire::Bounded,
    );
    let before = state(1, 0, None);
    let after = state(2, 1, Some(1));
    let actions = vec![
        PlayerActionWire::Bid {
            player: 0,
            tricks: 0,
        },
        PlayerActionWire::Bid {
            player: 0,
            tricks: 1,
        },
        PlayerActionWire::Bid {
            player: 0,
            tricks: 2,
        },
    ];
    let transition = TransitionWire {
        before_state_id: before.state_id,
        action: EnvironmentActionWire::Player(PlayerActionWire::Bid {
            player: 0,
            tricks: 1,
        }),
        after,
        round_scores: Vec::new(),
        game_outcome: None,
        diffs: vec![StateDiffWire {
            path: "bids[0]".to_owned(),
            before: Some("none".to_owned()),
            after: Some("1".to_owned()),
            rule_ids: vec!["R-BID-004".to_owned()],
        }],
        rule_ids: vec!["R-BID-004".to_owned()],
    };
    let trace = TraceWire {
        context: result_context.clone(),
        fixture_id: "unrestricted-total-bid".to_owned(),
        initial: before.clone(),
        steps: vec![TraceStepWire {
            index: 0,
            observation: Some(observation(&before, 0)),
            legal_actions: actions.clone(),
            transition: transition.clone(),
        }],
        cycle_start: None,
    };
    EvidenceBundleWire {
        fixture: FixtureWire {
            context: fixture_context,
            fixture_id: "unrestricted-total-bid".to_owned(),
            description: "The bid total is intentionally unrestricted.".to_owned(),
            state: before.clone(),
            observations: vec![observation(&before, 0), observation(&before, 1)],
            legal_actions: actions,
            chance_actions: vec![ChanceActionWire {
                deck: vec![0, 1, 2, 3, 4, 5],
                provenance: ChanceProvenanceWire::Seeded {
                    seed: 42,
                    deal_ordinal: 0,
                },
            }],
            expected_transition: Some(transition),
        },
        result: SolverResultWire {
            context: result_context,
            status: SolverStatusWire::Satisfied,
            summary: "Alloy found the expected bid witness.".to_owned(),
            bindings: vec![PrologBindingWire {
                variable: "Bid".to_owned(),
                term: "1".to_owned(),
            }],
            trace: Some(trace),
            statistics: StatisticsWire {
                states: Some(2),
                transitions: Some(1),
                max_depth: Some(1),
                duplicates: Some(0),
                duration_ms: 7,
                backend: vec![StatisticWire {
                    name: "sat_variables".to_owned(),
                    value: 19,
                }],
            },
            raw_diagnostics: vec![RawDiagnosticWire {
                stream: "stdout".to_owned(),
                severity: "info".to_owned(),
                text: "SAT".to_owned(),
            }],
            counterexample_diffs: vec![ProjectionDiffWire {
                projection: ProjectionKindWire::LegalActions,
                path: "legal_actions".to_owned(),
                expected: "bid(0..=2)".to_owned(),
                actual: "bid(0..=1)".to_owned(),
                rule_ids: vec!["R-BID-004".to_owned()],
            }],
        },
    }
}

#[test]
fn phon_roundtrip_covers_fixture_trace_and_solver_shapes() {
    let bundle = sample_bundle();
    let bytes = phon::api::encode(&bundle).unwrap();
    let decoded: EvidenceBundleWire = phon::api::decode(&bytes).unwrap();

    assert_eq!(decoded, bundle);
    assert_eq!(
        ValidatedEvidence::try_from(decoded).unwrap().wire(),
        &bundle
    );

    let score = RoundScoreWire {
        player: 0,
        bid: 2,
        tricks_won: 2,
        points: 12,
        payment_cents: 0,
        score_rule_id: "R-SCORE-004".to_owned(),
        money_rule_id: "R-MONEY-002".to_owned(),
    };
    let score_bytes = phon::api::encode(&score).unwrap();
    assert_eq!(
        phon::api::decode::<RoundScoreWire>(&score_bytes).unwrap(),
        score
    );

    let outcome = GameOutcomeWire {
        scores: vec![12, 12],
        pot_cents: 101,
        winners: vec![0, 1],
        pot_shares: vec![
            PotShareWire {
                player: 0,
                cents: 50,
            },
            PotShareWire {
                player: 1,
                cents: 50,
            },
        ],
        remainder_cents: 1,
    };
    let outcome_bytes = phon::api::encode(&outcome).unwrap();
    assert_eq!(
        phon::api::decode::<GameOutcomeWire>(&outcome_bytes).unwrap(),
        outcome
    );
}

#[test]
fn semantic_identity_prevents_cross_contract_confusion() {
    let mut bundle = sample_bundle();
    assert!(ValidatedEvidence::try_from(bundle.clone()).is_ok());

    // Model/backend/confidence may differ because cross-model comparison is the
    // point; the contracts that give values meaning may not.
    assert_ne!(
        bundle.fixture.context.model.model_id,
        bundle.result.context.model.model_id
    );
    assert_ne!(
        bundle.fixture.context.backend,
        bundle.result.context.backend
    );
    assert_ne!(
        bundle.fixture.context.confidence,
        bundle.result.context.confidence
    );
    bundle.result.context.model.scoring_semantic_hash = hash(99);
    if let Some(trace) = &mut bundle.result.trace {
        trace.context.model.scoring_semantic_hash = hash(99);
    }
    let error = ValidatedEvidence::try_from(bundle).unwrap_err();
    assert_eq!(error.path(), "result.context");
    assert!(error.message().contains("semantically compatible"));
}

#[test]
fn fixture_compatibility_revalidates_visibility_and_finite_domains() {
    let mut leaked = sample_bundle();
    leaked.fixture.observations[0].own_hand.push(2);
    let error = ValidatedEvidence::try_from(leaked).unwrap_err();
    assert_eq!(error.path(), "fixture.observations[0]");

    let mut duplicate_card = sample_bundle();
    duplicate_card.fixture.chance_actions[0].deck[5] = 4;
    let error = ValidatedEvidence::try_from(duplicate_card).unwrap_err();
    assert!(error.path().starts_with("fixture.chance_actions[0].deck"));

    let mut missing_hash = sample_bundle();
    missing_hash.fixture.context.model.rules_semantic_hash = SemanticHashWire([0; 32]);
    let error = ValidatedEvidence::try_from(missing_hash).unwrap_err();
    assert_eq!(error.path(), "fixture.context.model.rules_semantic_hash");
}

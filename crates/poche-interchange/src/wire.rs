use facet::Facet;

/// A 256-bit identity for canonical semantic content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct SemanticHashWire(pub [u8; 32]);

impl SemanticHashWire {
    /// Return whether this is the reserved missing/unknown hash.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0 == [0; 32]
    }
}

/// Identity of one executable model and all contracts affecting its meaning.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ModelIdentityWire {
    /// Stable model family, such as `poche-rust-formal`.
    pub model_id: String,
    /// Source revision or release identifier.
    pub model_revision: String,
    /// Stable root schema identifier.
    pub schema_id: String,
    /// Hash of the root schema and semantic refinements.
    pub schema_semantic_hash: SemanticHashWire,
    /// Rules document/catalog revision.
    pub rules_revision: String,
    /// Hash of normalized rules and decisions.
    pub rules_semantic_hash: SemanticHashWire,
    /// Hash distinguishing visibility/observation contracts.
    pub observation_semantic_hash: SemanticHashWire,
    /// Hash distinguishing score and money contracts.
    pub scoring_semantic_hash: SemanticHashWire,
}

/// Exact finite scope to which evidence applies.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ScopeWire {
    /// Stable scope name, such as `micro-2p-2s-3r-2h`.
    pub scope_id: String,
    /// Number of seats.
    pub player_count: u8,
    /// Number of suits.
    pub suit_count: u8,
    /// Number of ranks in each suit.
    pub ranks_per_suit: u8,
    /// Total distinct cards.
    pub deck_size: u8,
    /// Cards dealt to each player for this state/fixture.
    pub cards_per_player: u8,
    /// Face-up trump-card count.
    pub trump_card_count: u8,
    /// Undealt-card count immediately after a deal.
    pub undealt_card_count: u8,
    /// Whether every state/action in this declared scope was explored.
    pub exhaustive: bool,
}

/// Backend family that produced an evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum BackendKindWire {
    /// Conventional Rust oracle execution.
    RustOracle,
    /// Strict Facet/Weavy Rust model execution.
    RustFormal,
    /// Explicit-state Rust graph exploration.
    RustExplicit,
    /// Alloy analyzer.
    Alloy,
    /// `NuSMV` model checker.
    NuSmv,
    /// Scryer Prolog query engine.
    ScryerProlog,
}

/// Exact backend implementation and version.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct BackendWire {
    /// Backend family.
    pub kind: BackendKindWire,
    /// Native tool/crate version.
    pub version: String,
}

/// Strength and interpretation of one result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ConfidenceKindWire {
    /// Guaranteed by construction of a refined type.
    Structural,
    /// Every state and action in the named finite scope was explored.
    Exhaustive,
    /// Checked only within a named relational/finite bound.
    Bounded,
    /// Established by a symbolic temporal/model-checking run.
    Symbolic,
    /// Answered by a native relational query.
    Queried,
    /// Observed in generated/random testing.
    Sampled,
    /// Observed in one or more concrete simulations.
    Simulated,
}

/// A confidence classification plus a human-readable scope limitation.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ConfidenceWire {
    /// Evidence strength.
    pub kind: ConfidenceKindWire,
    /// Exact qualification/cutoff statement.
    pub qualification: String,
}

/// Whether an evidence subject is a fixture, property, or query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum SubjectKindWire {
    /// Concrete scenario/fixture.
    Scenario,
    /// Boolean or temporal property.
    Property,
    /// Relational query.
    Query,
}

/// Stable evidence subject identity.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SubjectWire {
    /// Subject category.
    pub kind: SubjectKindWire,
    /// Stable property, query, or scenario ID.
    pub id: String,
}

/// One rule and its human source anchor.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RuleRefWire {
    /// Stable rule ID from the coverage ledger.
    pub rule_id: String,
    /// Human-readable source anchor.
    pub source: String,
}

/// Identity carried by every fixture, trace, and solver result.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct EvidenceContextWire {
    /// Executable semantic model.
    pub model: ModelIdentityWire,
    /// Named finite scope.
    pub scope: ScopeWire,
    /// Producing backend and version.
    pub backend: BackendWire,
    /// Scenario, property, or query being evaluated.
    pub subject: SubjectWire,
    /// Rule origins used by the subject.
    pub rules: Vec<RuleRefWire>,
    /// Evidence strength and limits.
    pub confidence: ConfidenceWire,
}

/// Poche state-machine phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PhaseWire {
    /// Waiting for an explicit deck/chance action.
    AwaitingDeal,
    /// Collecting fixed bids.
    Bidding,
    /// Playing cards into tricks.
    Playing,
    /// Deterministic round settlement.
    Scoring,
    /// Absorbing terminal state.
    Finished,
}

/// One card play retained in table or trick history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct PlayedCardWire {
    /// Seat that played the card.
    pub player: u8,
    /// Dense card identity within the declared scope.
    pub card: u8,
}

/// Complete hidden game state in model-neutral finite values.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StateWire {
    /// Stable state identity within a trace/graph.
    pub state_id: u64,
    /// Current phase.
    pub phase: PhaseWire,
    /// Current dealer seat.
    pub dealer: u8,
    /// Acting player, absent for chance/environment/finished turns.
    pub actor: Option<u8>,
    /// Current hand size in the `1..m..1` schedule.
    pub hand_size: u8,
    /// Private hands indexed by seat.
    pub hands: Vec<Vec<u8>>,
    /// Face-up trump card, when the scope/round has one.
    pub trump: Option<u8>,
    /// Cards not dealt or exposed.
    pub undealt: Vec<u8>,
    /// In-progress trick in play order.
    pub current_trick: Vec<PlayedCardWire>,
    /// Completed tricks in round order and play order.
    pub completed_tricks: Vec<Vec<PlayedCardWire>>,
    /// Fixed bids indexed by seat; absent until made.
    pub bids: Vec<Option<u8>>,
    /// Tricks won in the current round, indexed by seat.
    pub tricks_won: Vec<u8>,
    /// Cumulative game scores indexed by seat.
    pub cumulative_scores: Vec<i32>,
    /// Communal pot in cents.
    pub pot_cents: u32,
}

/// Information visible to exactly one player.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ObservationWire {
    /// State being observed.
    pub state_id: u64,
    /// Viewing seat.
    pub viewer: u8,
    /// Public phase.
    pub phase: PhaseWire,
    /// Public dealer.
    pub dealer: u8,
    /// Public actor, when player-owned.
    pub actor: Option<u8>,
    /// Only the viewer's private cards.
    pub own_hand: Vec<u8>,
    /// Public hand sizes for all seats, without other card identities.
    pub hand_counts: Vec<u8>,
    /// Public trump card.
    pub trump: Option<u8>,
    /// Public current trick.
    pub current_trick: Vec<PlayedCardWire>,
    /// Public bids; absent until announced.
    pub bids: Vec<Option<u8>>,
    /// Public tricks won by seat.
    pub tricks_won: Vec<u8>,
    /// Public cumulative scores.
    pub cumulative_scores: Vec<i32>,
    /// Public pot in cents.
    pub pot_cents: u32,
}

/// Player-owned Poche action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PlayerActionWire {
    /// Announce a fixed trick bid.
    Bid {
        /// Acting seat.
        player: u8,
        /// Bid in whole tricks.
        tricks: u8,
    },
    /// Play a card from the acting player's hand.
    Play {
        /// Acting seat.
        player: u8,
        /// Dense card identity.
        card: u8,
    },
}

/// Replay provenance for an explicit chance action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChanceProvenanceWire {
    /// Deck order was directly supplied.
    Explicit,
    /// Deck order was deterministically derived from these values.
    Seeded {
        /// Stable replay seed.
        seed: u64,
        /// Zero-based deal ordinal.
        deal_ordinal: u32,
    },
}

/// Chance-owned action carrying a complete deck permutation.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ChanceActionWire {
    /// Complete dense-card order.
    pub deck: Vec<u8>,
    /// Replay provenance outside semantic state.
    pub provenance: ChanceProvenanceWire,
}

/// Mutually exclusive transition input ownership.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum EnvironmentActionWire {
    /// Player policy choice.
    Player(PlayerActionWire),
    /// Explicit environment chance result.
    Chance(ChanceActionWire),
    /// Deterministic environment settlement.
    Settle,
}

/// One player's raw score and payment at a round boundary.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RoundScoreWire {
    /// Scored seat.
    pub player: u8,
    /// Fixed bid.
    pub bid: u8,
    /// Tricks actually won.
    pub tricks_won: u8,
    /// Raw Poche points for the round.
    pub points: u16,
    /// Payment into the pot in cents.
    pub payment_cents: u32,
    /// Rule producing points.
    pub score_rule_id: String,
    /// Rule producing payment.
    pub money_rule_id: String,
}

/// One final pot share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct PotShareWire {
    /// Winning seat.
    pub player: u8,
    /// Assigned cents.
    pub cents: u32,
}

/// Final score and money outcome, kept as distinct projections.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GameOutcomeWire {
    /// Final cumulative points by seat.
    pub scores: Vec<u16>,
    /// Total communal pot before division.
    pub pot_cents: u32,
    /// All seats tied for highest score.
    pub winners: Vec<u8>,
    /// Assigned pot shares.
    pub pot_shares: Vec<PotShareWire>,
    /// Intentionally unassigned indivisible-cent remainder.
    pub remainder_cents: u32,
}

/// One field-level state change.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StateDiffWire {
    /// Stable field/index path.
    pub path: String,
    /// Canonical pre-transition value, absent for insertion.
    pub before: Option<String>,
    /// Canonical post-transition value, absent for removal.
    pub after: Option<String>,
    /// Rules responsible for the change.
    pub rule_ids: Vec<String>,
}

/// Result of one environment transition.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct TransitionWire {
    /// Predecessor state ID.
    pub before_state_id: u64,
    /// Explicit player/chance/environment action.
    pub action: EnvironmentActionWire,
    /// Complete successor state.
    pub after: StateWire,
    /// Raw round scores, empty away from a round boundary.
    pub round_scores: Vec<RoundScoreWire>,
    /// Final outcome, present exactly at terminal transition.
    pub game_outcome: Option<GameOutcomeWire>,
    /// Canonical state differences.
    pub diffs: Vec<StateDiffWire>,
    /// Rules applied by the transition.
    pub rule_ids: Vec<String>,
}

/// One replayable trace step.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct TraceStepWire {
    /// Zero-based transition index.
    pub index: u32,
    /// Observation available to the acting player, when player-owned.
    pub observation: Option<ObservationWire>,
    /// Complete legal player action set before the transition.
    pub legal_actions: Vec<PlayerActionWire>,
    /// Applied transition and successor.
    pub transition: TransitionWire,
}

/// Replayable finite prefix and optional liveness cycle.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct TraceWire {
    /// Full semantic/evidence identity.
    pub context: EvidenceContextWire,
    /// Concrete fixture that seeded the trace.
    pub fixture_id: String,
    /// Initial complete state.
    pub initial: StateWire,
    /// Ordered trace steps.
    pub steps: Vec<TraceStepWire>,
    /// First step in a repeating lasso cycle, absent for a finite trace.
    pub cycle_start: Option<u32>,
}

/// One variable binding returned by Prolog.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct PrologBindingWire {
    /// Query variable name.
    pub variable: String,
    /// Canonical Prolog term text.
    pub term: String,
}

/// Solver/checker outcome independent of tool phrasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum SolverStatusWire {
    /// Witness/query/property succeeded.
    Satisfied,
    /// No witness or a false query/property.
    Unsatisfied,
    /// Property was refuted with a counterexample.
    Counterexample,
    /// Backend did not establish a definitive answer.
    Unknown,
    /// Invocation, parsing, or model execution failed.
    Error,
}

/// Named numeric backend statistic.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StatisticWire {
    /// Stable metric name.
    pub name: String,
    /// Integer metric value.
    pub value: u64,
}

/// Common and backend-specific run statistics.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StatisticsWire {
    /// Reachable/considered states, when reported.
    pub states: Option<u64>,
    /// Explored transitions, when reported.
    pub transitions: Option<u64>,
    /// Maximum explored depth, when reported.
    pub max_depth: Option<u64>,
    /// Duplicate states, when reported.
    pub duplicates: Option<u64>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Backend-specific numeric metrics.
    pub backend: Vec<StatisticWire>,
}

/// Raw native-tool diagnostic preserved beside normalized output.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RawDiagnosticWire {
    /// `stdout`, `stderr`, or an adapter-defined source.
    pub stream: String,
    /// `info`, `warning`, or `error`.
    pub severity: String,
    /// Unmodified diagnostic text.
    pub text: String,
}

/// Semantic projection in which a counterexample exposes a disagreement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ProjectionKindWire {
    /// Complete hidden semantic state.
    State,
    /// One viewer's information projection.
    Observation,
    /// Legal action set or ownership surface.
    LegalActions,
    /// State-transition successor or provenance.
    Transition,
    /// Raw round-score/payment event.
    RoundScore,
    /// Terminal score/money outcome.
    GameOutcome,
}

/// Expected-versus-actual projection difference carried by a counterexample.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ProjectionDiffWire {
    /// Projection family.
    pub projection: ProjectionKindWire,
    /// Stable field/index or expression path.
    pub path: String,
    /// Correct model projection.
    pub expected: String,
    /// Defective/backend projection.
    pub actual: String,
    /// Rules that make the projections disagree.
    pub rule_ids: Vec<String>,
}

/// Normalized result from any native or Rust backend.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SolverResultWire {
    /// Full semantic/evidence identity.
    pub context: EvidenceContextWire,
    /// Normalized outcome.
    pub status: SolverStatusWire,
    /// Short adapter-produced summary.
    pub summary: String,
    /// Relational bindings (primarily Prolog).
    pub bindings: Vec<PrologBindingWire>,
    /// Replayable witness or counterexample.
    pub trace: Option<TraceWire>,
    /// Run statistics.
    pub statistics: StatisticsWire,
    /// Original tool output and adapter diagnostics.
    pub raw_diagnostics: Vec<RawDiagnosticWire>,
    /// Typed expected/actual differences explaining a counterexample.
    pub counterexample_diffs: Vec<ProjectionDiffWire>,
}

/// Model-neutral fixture independently consumable by native adapters.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct FixtureWire {
    /// Full semantic/evidence identity expected for this fixture.
    pub context: EvidenceContextWire,
    /// Stable fixture ID from the oracle inventory.
    pub fixture_id: String,
    /// Human-readable intent.
    pub description: String,
    /// Complete initial/checkpoint state.
    pub state: StateWire,
    /// Expected observations for selected viewers.
    pub observations: Vec<ObservationWire>,
    /// Expected legal player actions.
    pub legal_actions: Vec<PlayerActionWire>,
    /// Candidate/expected explicit chance inputs.
    pub chance_actions: Vec<ChanceActionWire>,
    /// Expected transition, when this is a transition fixture.
    pub expected_transition: Option<TransitionWire>,
}

/// Root Phon payload pairing a fixture with one backend result.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct EvidenceBundleWire {
    /// Fixture supplied to an adapter.
    pub fixture: FixtureWire,
    /// Backend result for that fixture/subject.
    pub result: SolverResultWire,
}

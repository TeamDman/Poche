use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::{Mutex, OnceLock};

use facet::Facet;
use poche_formal::{
    ComputationKind, Diagnostic, EnumType, FieldType, FormalProgram, GraphBuilder, IntRange,
    Origin, RecordType, SourceRef, Type, Value,
};

use crate::{Bid, Card, ModelError, RoundId};

/// Named pure kernels used by the strict transition system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum KernelId {
    /// Bid lies within the scheduled hand size.
    BidLegal,
    /// A selected card satisfies follow-suit.
    FollowSuitLegal,
    /// The second card defeats the lead card.
    SecondCardWins,
    /// Round outcome, points, and payment.
    RoundScore,
    /// Whether settlement follows the final scheduled round.
    FinalRound,
    /// Complete maximum-score winner mask.
    WinnerMask,
}

/// Inspectable catalog entry for a pure Weavy kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormalKernelInfo {
    /// Stable kernel ID.
    pub id: KernelId,
    /// Output type in the restricted dialect.
    pub output_type: Type,
    /// Rules represented by instruction origins.
    pub rule_ids: Vec<String>,
    /// Number of lowered instructions with source origins.
    pub instruction_count: usize,
}

/// Build and report every pure kernel used by transition semantics.
///
/// # Errors
///
/// Returns the source-oriented lowering diagnostic for any invalid graph.
pub fn formal_kernel_catalog() -> Result<Vec<FormalKernelInfo>, Diagnostic> {
    [
        (KernelId::BidLegal, bid_program as fn() -> _),
        (KernelId::FollowSuitLegal, follow_suit_program),
        (KernelId::SecondCardWins, second_wins_program),
        (KernelId::RoundScore, round_score_program),
        (KernelId::FinalRound, final_round_program),
        (KernelId::WinnerMask, winner_mask_program),
    ]
    .into_iter()
    .map(|(id, build)| {
        let program = build()?;
        let origins: Vec<&Origin> = program.origins().collect();
        let rule_ids = origins
            .iter()
            .map(|origin| origin.rule_id().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(FormalKernelInfo {
            id,
            output_type: program.output_type().clone(),
            rule_ids,
            instruction_count: origins.len(),
        })
    })
    .collect()
}

pub(crate) fn bid_legal(bid: Bid, round: RoundId) -> Result<bool, ModelError> {
    cached_evaluation(&BID_RESULTS, (bid, round), || {
        evaluate_bool(
            bid_program()?,
            [("bid", bid.get()), ("hand-size", round.hand_size())],
        )
    })
}

pub(crate) fn follow_suit_legal(
    hand_has_lead: bool,
    selected: Card,
    lead: Card,
) -> Result<bool, ModelError> {
    cached_evaluation(
        &FOLLOW_SUIT_RESULTS,
        (hand_has_lead, selected, lead),
        || {
            let inputs = BTreeMap::from([
                ("hand-has-lead".to_owned(), Value::Bool(hand_has_lead)),
                (
                    "selected-suit".to_owned(),
                    Value::Integer(i32::from(selected.suit())),
                ),
                (
                    "lead-suit".to_owned(),
                    Value::Integer(i32::from(lead.suit())),
                ),
            ]);
            let value = follow_suit_program()?.evaluate(&inputs)?;
            value_bool(&value)
        },
    )
}

pub(crate) fn second_card_wins(lead: Card, second: Card, trump: Card) -> Result<bool, ModelError> {
    cached_evaluation(&SECOND_WINS_RESULTS, (lead, second, trump), || {
        let inputs = BTreeMap::from([
            (
                "lead-suit".to_owned(),
                Value::Integer(i32::from(lead.suit())),
            ),
            (
                "lead-rank".to_owned(),
                Value::Integer(i32::from(lead.rank())),
            ),
            (
                "second-suit".to_owned(),
                Value::Integer(i32::from(second.suit())),
            ),
            (
                "second-rank".to_owned(),
                Value::Integer(i32::from(second.rank())),
            ),
            (
                "trump-suit".to_owned(),
                Value::Integer(i32::from(trump.suit())),
            ),
        ]);
        let value = second_wins_program()?.evaluate(&inputs)?;
        value_bool(&value)
    })
}

pub(crate) fn round_score(
    bid: Bid,
    tricks: u8,
    round: RoundId,
) -> Result<(u8, u8, u8), ModelError> {
    cached_evaluation(&ROUND_SCORE_RESULTS, (bid, tricks, round), || {
        let inputs = integer_inputs([
            ("bid", bid.get()),
            ("tricks", tricks),
            ("hand-size", round.hand_size()),
        ]);
        let Value::Record { fields, .. } = round_score_program()?.evaluate(&inputs)? else {
            return Err(ModelError::Invariant("formal score output record"));
        };
        let [outcome, points, payment]: [Value; 3] = fields
            .try_into()
            .map_err(|_| ModelError::Invariant("formal score output arity"))?;
        let Value::Enumeration { ordinal, .. } = outcome else {
            return Err(ModelError::Invariant("formal score outcome enum"));
        };
        let Value::Integer(points) = points else {
            return Err(ModelError::Invariant("formal score points integer"));
        };
        let Value::Integer(payment) = payment else {
            return Err(ModelError::Invariant("formal score payment integer"));
        };
        Ok((
            u8::try_from(ordinal).map_err(|_| ModelError::Invariant("score outcome ordinal"))?,
            u8::try_from(points).map_err(|_| ModelError::Invariant("score points range"))?,
            u8::try_from(payment).map_err(|_| ModelError::Invariant("score payment range"))?,
        ))
    })
}

pub(crate) fn final_round(round: RoundId) -> Result<bool, ModelError> {
    cached_evaluation(&FINAL_ROUND_RESULTS, round, || {
        evaluate_bool(final_round_program()?, [("round", round as u8)])
    })
}

pub(crate) fn winner_mask(scores: [u8; 2]) -> Result<[bool; 2], ModelError> {
    cached_evaluation(&WINNER_MASK_RESULTS, scores, || {
        let result = winner_mask_program()?.evaluate(&integer_inputs([
            ("score-zero", scores[0]),
            ("score-one", scores[1]),
        ]))?;
        let Value::Record { fields, .. } = result else {
            return Err(ModelError::Invariant("formal winner output record"));
        };
        let [zero, one]: [Value; 2] = fields
            .try_into()
            .map_err(|_| ModelError::Invariant("formal winner output arity"))?;
        Ok([value_bool(&zero)?, value_bool(&one)?])
    })
}

type ResultCache<K, V> = OnceLock<Mutex<HashMap<K, Result<V, ModelError>>>>;

static BID_RESULTS: ResultCache<(Bid, RoundId), bool> = OnceLock::new();
static FOLLOW_SUIT_RESULTS: ResultCache<(bool, Card, Card), bool> = OnceLock::new();
static SECOND_WINS_RESULTS: ResultCache<(Card, Card, Card), bool> = OnceLock::new();
static ROUND_SCORE_RESULTS: ResultCache<(Bid, u8, RoundId), (u8, u8, u8)> = OnceLock::new();
static FINAL_ROUND_RESULTS: ResultCache<RoundId, bool> = OnceLock::new();
static WINNER_MASK_RESULTS: ResultCache<[u8; 2], [bool; 2]> = OnceLock::new();

fn cached_evaluation<K, V>(
    cache: &'static ResultCache<K, V>,
    key: K,
    evaluate: impl FnOnce() -> Result<V, ModelError>,
) -> Result<V, ModelError>
where
    K: Copy + Eq + Hash,
    V: Clone,
{
    let values = cache.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(result) = values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return result;
    }
    let result = evaluate();
    values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, result.clone());
    result
}

static BID_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();
static FOLLOW_SUIT_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();
static SECOND_WINS_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();
static ROUND_SCORE_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();
static FINAL_ROUND_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();
static WINNER_MASK_PROGRAM: OnceLock<Result<FormalProgram, Diagnostic>> = OnceLock::new();

fn bid_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&BID_PROGRAM, build_bid_legal)
}

fn follow_suit_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&FOLLOW_SUIT_PROGRAM, build_follow_suit)
}

fn second_wins_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&SECOND_WINS_PROGRAM, build_second_wins)
}

fn round_score_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&ROUND_SCORE_PROGRAM, build_round_score)
}

fn final_round_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&FINAL_ROUND_PROGRAM, build_final_round)
}

fn winner_mask_program() -> Result<&'static FormalProgram, Diagnostic> {
    cached(&WINNER_MASK_PROGRAM, build_winner_mask)
}

fn cached(
    slot: &'static OnceLock<Result<FormalProgram, Diagnostic>>,
    build: fn() -> Result<FormalProgram, Diagnostic>,
) -> Result<&'static FormalProgram, Diagnostic> {
    match slot.get_or_init(build) {
        Ok(program) => Ok(program),
        Err(error) => Err(error.clone()),
    }
}

fn build_bid_legal() -> Result<FormalProgram, Diagnostic> {
    let origin = rule_origin(
        "R-BID-002",
        275,
        "whole-number bid from zero through hand size",
        ComputationKind::LegalAction,
    );
    let bounded = Type::Integer(IntRange::new(0, 2).expect("static range is valid"));
    let mut graph = GraphBuilder::new();
    let bid = graph.input("bid", bounded.clone(), origin.clone())?;
    let hand = graph.input("hand-size", bounded, origin.clone())?;
    let legal = graph.less_equal(bid, hand, origin)?;
    graph.lower(legal)
}

fn build_follow_suit() -> Result<FormalProgram, Diagnostic> {
    let origin = rule_origin(
        "R-TRICK-005",
        292,
        "follower holding lead suit must follow",
        ComputationKind::LegalAction,
    );
    let mut graph = GraphBuilder::new();
    let suit = Type::Integer(IntRange::new(0, 1).expect("static range is valid"));
    let has_lead = graph.input("hand-has-lead", Type::Bool, origin.clone())?;
    let selected = graph.input("selected-suit", suit.clone(), origin.clone())?;
    let lead = graph.input("lead-suit", suit, origin.clone())?;
    let not_required = graph.not(has_lead, origin.clone())?;
    let follows = graph.equal(selected, lead, origin.clone())?;
    let legal = graph.fixed_any(vec![not_required, follows], origin)?;
    graph.lower(legal)
}

#[allow(
    clippy::too_many_lines,
    reason = "the explicit winner formula is kept together for audit against four trick rules"
)]
fn build_second_wins() -> Result<FormalProgram, Diagnostic> {
    let origin = rule_origin(
        "R-TRICK-007",
        296,
        "trump, then lead suit, then rank determines winner",
        ComputationKind::Transition,
    );
    let rank_origin = rule_origin(
        "R-TRICK-010",
        301,
        "ranks ascend within eligible suits",
        ComputationKind::Transition,
    );
    let suit = Type::Integer(IntRange::new(0, 1).expect("static range is valid"));
    let rank = Type::Integer(IntRange::new(0, 2).expect("static range is valid"));
    let mut graph = GraphBuilder::new();
    let lead_suit = graph.input("lead-suit", suit.clone(), origin.clone())?;
    let lead_rank = graph.input("lead-rank", rank.clone(), rank_origin.clone())?;
    let second_suit = graph.input("second-suit", suit.clone(), origin.clone())?;
    let second_rank = graph.input("second-rank", rank, rank_origin.clone())?;
    let trump_suit = graph.input("trump-suit", suit, origin.clone())?;
    let lead_trump = graph.equal(lead_suit, trump_suit, origin.clone())?;
    let second_trump = graph.equal(second_suit, trump_suit, origin.clone())?;
    let lead_not_trump = graph.not(lead_trump, origin.clone())?;
    let trump_advantage = graph.fixed_all(vec![second_trump, lead_not_trump], origin.clone())?;
    let same_suit = graph.equal(second_suit, lead_suit, origin.clone())?;
    let higher = graph.less(lead_rank, second_rank, rank_origin)?;
    let same_suit_higher = graph.fixed_all(vec![same_suit, higher], origin.clone())?;
    let result = graph.fixed_any(vec![trump_advantage, same_suit_higher], origin)?;
    graph.lower(result)
}

#[allow(
    clippy::too_many_lines,
    reason = "the scoring decision table is intentionally explicit and rule-originated"
)]
fn build_round_score() -> Result<FormalProgram, Diagnostic> {
    let compare = rule_origin(
        "R-SCORE-001",
        311,
        "compare fixed bid with trick count",
        ComputationKind::Scoring,
    );
    let miss = rule_origin(
        "R-SCORE-002",
        317,
        "missed bid scores zero",
        ComputationKind::Scoring,
    );
    let exact = rule_origin(
        "R-SCORE-003",
        317,
        "exact partial bid scores ten plus bid",
        ComputationKind::Scoring,
    );
    let all = rule_origin(
        "R-SCORE-004",
        317,
        "all tricks scores twenty plus bid",
        ComputationKind::Scoring,
    );
    let payment = rule_origin(
        "R-MONEY-001",
        350,
        "missed bid pays ten cents",
        ComputationKind::Scoring,
    );
    let output = rule_origin(
        "R-SCORE-005",
        323,
        "round score contributes to cumulative score",
        ComputationKind::Scoring,
    );
    let small = Type::Integer(IntRange::new(0, 2).expect("static range is valid"));
    let point = Type::Integer(IntRange::new(0, 22).expect("static range is valid"));
    let cents = Type::Integer(IntRange::new(0, 10).expect("static range is valid"));
    let outcome = EnumType::new("RoundOutcome", ["Miss", "Exact", "AllTricks"])
        .expect("static enum is valid");
    let result = RecordType::new(
        "RoundScore",
        [
            FieldType::new("outcome", Type::Enumeration(outcome.clone()))
                .expect("static field is valid"),
            FieldType::new("points", point.clone()).expect("static field is valid"),
            FieldType::new("payment-cents", cents.clone()).expect("static field is valid"),
        ],
    )
    .expect("static record is valid");
    let mut graph = GraphBuilder::new();
    let bid = graph.input("bid", small.clone(), compare.clone())?;
    let tricks = graph.input("tricks", small.clone(), compare.clone())?;
    let hand = graph.input("hand-size", small.clone(), all.clone())?;
    let is_exact = graph.equal(bid, tricks, compare)?;
    let is_all = graph.equal(tricks, hand, all.clone())?;
    let zero_small = graph.constant(small.clone(), Value::Integer(0), exact.clone())?;
    let one_small = graph.constant(small, Value::Integer(1), exact.clone())?;
    let bid_zero = graph.equal(bid, zero_small, exact.clone())?;
    let bid_one = graph.equal(bid, one_small, exact.clone())?;

    let p0 = graph.constant(point.clone(), Value::Integer(0), miss.clone())?;
    let p10 = graph.constant(point.clone(), Value::Integer(10), exact.clone())?;
    let p11 = graph.constant(point.clone(), Value::Integer(11), exact.clone())?;
    let p12 = graph.constant(point.clone(), Value::Integer(12), exact.clone())?;
    let p20 = graph.constant(point.clone(), Value::Integer(20), all.clone())?;
    let p21 = graph.constant(point.clone(), Value::Integer(21), all.clone())?;
    let p22 = graph.constant(point, Value::Integer(22), all.clone())?;
    let exact_nonzero = graph.select(bid_one, p11, p12, exact.clone())?;
    let exact_points = graph.select(bid_zero, p10, exact_nonzero, exact.clone())?;
    let all_nonzero = graph.select(bid_one, p21, p22, all.clone())?;
    let all_points = graph.select(bid_zero, p20, all_nonzero, all.clone())?;
    let successful_points = graph.select(is_all, all_points, exact_points, all.clone())?;
    let points = graph.select(is_exact, successful_points, p0, output.clone())?;

    let c0 = graph.constant(cents.clone(), Value::Integer(0), payment.clone())?;
    let c10 = graph.constant(cents, Value::Integer(10), payment.clone())?;
    let payment_cents = graph.select(is_exact, c0, c10, payment)?;

    let miss_value = graph.enumeration(outcome.clone(), "Miss", miss)?;
    let exact_value = graph.enumeration(outcome.clone(), "Exact", exact.clone())?;
    let all_value = graph.enumeration(outcome, "AllTricks", all.clone())?;
    let successful_outcome = graph.select(is_all, all_value, exact_value, all)?;
    let outcome_value = graph.select(is_exact, successful_outcome, miss_value, exact)?;
    let record = graph.record(result, vec![outcome_value, points, payment_cents], output)?;
    graph.lower(record)
}

fn build_final_round() -> Result<FormalProgram, Diagnostic> {
    let origin = rule_origin(
        "R-ADVANCE-003",
        363,
        "finish after the descending one-card round",
        ComputationKind::Transition,
    );
    let round = Type::Integer(IntRange::new(0, 2).expect("static range is valid"));
    let mut graph = GraphBuilder::new();
    let value = graph.input("round", round.clone(), origin.clone())?;
    let final_value = graph.constant(round, Value::Integer(2), origin.clone())?;
    let result = graph.equal(value, final_value, origin)?;
    graph.lower(result)
}

fn build_winner_mask() -> Result<FormalProgram, Diagnostic> {
    let origin = rule_origin(
        "R-FINISH-003",
        369,
        "all maximum-score players win",
        ComputationKind::Property,
    );
    let score = Type::Integer(IntRange::new(0, 64).expect("static range is valid"));
    let result = RecordType::new(
        "WinnerMask",
        [
            FieldType::new("zero", Type::Bool).expect("static field is valid"),
            FieldType::new("one", Type::Bool).expect("static field is valid"),
        ],
    )
    .expect("static record is valid");
    let mut graph = GraphBuilder::new();
    let zero = graph.input("score-zero", score.clone(), origin.clone())?;
    let one = graph.input("score-one", score, origin.clone())?;
    let zero_less = graph.less(zero, one, origin.clone())?;
    let one_less = graph.less(one, zero, origin.clone())?;
    let zero_wins = graph.not(zero_less, origin.clone())?;
    let one_wins = graph.not(one_less, origin.clone())?;
    let record = graph.record(result, vec![zero_wins, one_wins], origin)?;
    graph.lower(record)
}

fn evaluate_bool<const N: usize>(
    program: &FormalProgram,
    inputs: [(&str, u8); N],
) -> Result<bool, ModelError> {
    let value = program.evaluate(&integer_inputs(inputs))?;
    value_bool(&value)
}

fn value_bool(value: &Value) -> Result<bool, ModelError> {
    if let Value::Bool(value) = value {
        Ok(*value)
    } else {
        Err(ModelError::Invariant("formal Boolean output"))
    }
}

fn integer_inputs<const N: usize>(inputs: [(&str, u8); N]) -> BTreeMap<String, Value> {
    inputs
        .into_iter()
        .map(|(name, value)| (name.to_owned(), Value::Integer(i32::from(value))))
        .collect()
}

fn rule_origin(rule_id: &str, line: u32, clause: &str, computation: ComputationKind) -> Origin {
    Origin::new(
        rule_id,
        SourceRef::new("docs/main.typ", line, clause),
        computation,
    )
}

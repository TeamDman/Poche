// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounded structural agreement between the conventional Rust and Alloy
//! oracles. Alloy atom names are unstable, so the fixture declares semantic
//! role labels and this adapter compares the resulting relational projection.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;

use poche_native_tools::{
    AlloyCommandExpectation, AlloyCommandKind, AlloyOutcome, NativeDisposition, run_alloy_suite,
};
use poche_oracle_rust::{
    Action, BidOutcome, Card, DECK_SIZE, DeckOrder, Game, GameState, PlayedCard, Rank,
    RuleViolation, Seat, Suit, score_round, trick_winner,
};
use serde_json::{Map, Value};

const SUITE_ID: &str = "alloy-conformance";
const MODEL_PATH: &str = "models/alloy/conformance.als";
const REQUIRED_SCOPE_FRAGMENTS: [&str; 10] = [
    "7 Int",
    "exactly 2 Player",
    "exactly 52 Card",
    "exactly 1 Round",
    "exactly 6 Step",
    "exactly 0 GameResult",
    "exactly 0 FirstJackSelection",
    "exactly 0 HighCardProcess",
    "exactly 0 HighCardDraw",
    "for 6",
];

/// Successful Rust/Alloy bounded-conformance evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlloyConformanceReport {
    /// Number of commands whose typed SAT polarity was checked.
    pub command_count: usize,
    /// Canonical valid structural instances compared to Rust.
    pub valid_instances: usize,
    /// Individually named malformed structures rejected by the core facts.
    pub invalid_structures_rejected: usize,
    /// Aggregate assertions checked without a counterexample.
    pub assertions_checked: usize,
    /// Weakened-rule witnesses admitted and paired with Rust discrimination.
    pub controlled_defect_witnesses: usize,
    /// Semantic relation groups compared in the canonical instance.
    pub projection_groups_compared: usize,
    /// Exact source command and scope retained for every result.
    pub command_scopes: Vec<String>,
    /// Explicit boundedness and semantic-boundary qualifications.
    pub limitations: Vec<&'static str>,
}

/// Native execution, receipt parsing, scope, or semantic mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlloyConformanceError(String);

impl fmt::Display for AlloyConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AlloyConformanceError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RoundProjection {
    dealer: u8,
    first: u8,
    low_lead: Card,
    high_follow: Card,
    trump: Card,
    hands: [BTreeSet<Card>; 2],
    bids: [u8; 2],
    leader: u8,
    winner: u8,
    plays: [Card; 2],
    scores: [u16; 2],
    captured: [BTreeSet<Card>; 2],
    restored: BTreeSet<Card>,
}

/// Run the handwritten Alloy fixture suite and compare its canonical witness
/// with an independently executed conventional Rust round.
///
/// # Errors
///
/// Returns a diagnostic if Alloy is unavailable, a named command has the
/// wrong bounded result, its scope is lost, the receipt is malformed, or any
/// selected relation differs from Rust.
pub fn compare_rust_alloy(root: &Path) -> Result<AlloyConformanceReport, AlloyConformanceError> {
    let expectations = command_expectations();
    let native = run_alloy_suite(root, SUITE_ID, Path::new(MODEL_PATH), &expectations);
    if native.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "native Alloy suite was {:?}: {} ({})",
            native.disposition,
            native.diagnostic,
            native.evidence_directory.display()
        )));
    }
    if native.results.len() != expectations.len() {
        return Err(problem(format!(
            "expected {} Alloy results, received {}",
            expectations.len(),
            native.results.len()
        )));
    }

    let mut scopes = Vec::with_capacity(native.results.len());
    for result in &native.results {
        let source = result.command_source.as_ref().ok_or_else(|| {
            problem(format!(
                "{} has no receipt-derived source scope",
                result.name
            ))
        })?;
        for required in REQUIRED_SCOPE_FRAGMENTS {
            if !source.contains(required) {
                return Err(problem(format!(
                    "{} scope omitted {required:?}: {source:?}",
                    result.name
                )));
            }
        }
        if !source.contains("exactly 1 Trick") && !source.contains("exactly 2 Trick") {
            return Err(problem(format!(
                "{} scope omitted an exact Trick bound: {source:?}",
                result.name
            )));
        }
        scopes.push(source.clone());
    }

    let receipt_path = native.evidence_directory.join("receipt.json");
    let receipt = fs::read_to_string(&receipt_path).map_err(|error| {
        problem(format!(
            "could not read canonical Alloy receipt {}: {error}",
            receipt_path.display()
        ))
    })?;
    let receipt: Value = serde_json::from_str(&receipt)
        .map_err(|error| problem(format!("Alloy receipt is not valid JSON: {error}")))?;
    let alloy = alloy_projection(&receipt)?;
    let rust = rust_projection()?;
    compare_projection(&rust, &alloy)?;
    discriminate_rust_structures_and_defects()?;

    Ok(AlloyConformanceReport {
        command_count: expectations.len(),
        valid_instances: 1,
        invalid_structures_rejected: 5,
        assertions_checked: 1,
        controlled_defect_witnesses: 3,
        projection_groups_compared: 13,
        command_scopes: scopes,
        limitations: vec![
            "Every Alloy result is bounded to the exact scope retained in command_scopes; it is not an unbounded proof.",
            "The canonical comparison projects one complete two-player, one-card round rather than requiring Alloy and Rust internal state layouts to match.",
            "Alloy structural facts reject impossible relational instances; Rust primarily prevents equivalent construction and validates every card-owning runtime phase.",
        ],
    })
}

fn command_expectations() -> Vec<AlloyCommandExpectation> {
    use AlloyCommandKind::{Assertion, Witness};
    use AlloyOutcome::{Sat, Unsat};
    [
        ("CanonicalOneCardRustFixture", Witness, Sat),
        ("InvalidDuplicateHandCard", Witness, Unsat),
        ("InvalidTrumpInHand", Witness, Unsat),
        ("InvalidFollowSuitPlay", Witness, Unsat),
        ("InvalidWinner", Witness, Unsat),
        ("InvalidRoundScore", Witness, Unsat),
        ("CoreRejectsKnownInvalidStructures", Assertion, Unsat),
        ("UnrestrictedFollowSuitDefect", Witness, Sat),
        ("RankOnlyWinnerDefect", Witness, Sat),
        ("PartialAllTricksBonusDefect", Witness, Sat),
    ]
    .into_iter()
    .map(|(name, kind, outcome)| AlloyCommandExpectation {
        name: name.to_owned(),
        kind,
        outcome,
    })
    .collect()
}

fn rust_projection() -> Result<RoundProjection, AlloyConformanceError> {
    let dealer = seat(1)?;
    let first = seat(0)?;
    let low = Card::new(Suit::Clubs, Rank::Two);
    let high = Card::new(Suit::Clubs, Rank::Three);
    let trump = Card::new(Suit::Clubs, Rank::Four);
    let deck = DeckOrder::standard();
    let mut game = Game::<2>::new(dealer).map_err(rust_error("construct canonical game"))?;
    game = apply(&game, Action::Deal(deck.clone()), "deal canonical round")?;
    game.validate()
        .map_err(rust_error("validate canonical deal"))?;
    let hands = match game.state() {
        GameState::Bidding(state) => [
            state.hands[0].iter().collect(),
            state.hands[1].iter().collect(),
        ],
        _ => return Err(problem("canonical deal did not enter bidding")),
    };
    game = apply(
        &game,
        Action::Bid {
            player: first,
            tricks: 0,
        },
        "first canonical bid",
    )?;
    game = apply(
        &game,
        Action::Bid {
            player: dealer,
            tricks: 1,
        },
        "second canonical bid",
    )?;
    game = apply(
        &game,
        Action::Play {
            player: first,
            card: low,
        },
        "canonical lead",
    )?;
    game = apply(
        &game,
        Action::Play {
            player: dealer,
            card: high,
        },
        "canonical follow",
    )?;
    game.validate()
        .map_err(rust_error("validate canonical scoring state"))?;
    let captured = match game.state() {
        GameState::Scoring(state) => [
            state.captured[0].iter().collect(),
            state.captured[1].iter().collect(),
        ],
        _ => return Err(problem("canonical plays did not enter scoring")),
    };
    let settled = game
        .transition(Action::SettleRound)
        .map_err(rust_error("settle canonical round"))?;
    let scores = settled
        .round_scores
        .ok_or_else(|| problem("canonical settlement omitted round scores"))?
        .map(|score| score.points);
    let winner = trick_winner(
        trump.suit,
        &[
            PlayedCard {
                player: first,
                card: low,
            },
            PlayedCard {
                player: dealer,
                card: high,
            },
        ],
    )
    .ok_or_else(|| problem("Rust canonical trick had no winner"))?;

    Ok(RoundProjection {
        dealer: seat_number(dealer)?,
        first: seat_number(first)?,
        low_lead: low,
        high_follow: high,
        trump,
        hands,
        bids: [0, 1],
        leader: seat_number(first)?,
        winner: seat_number(winner)?,
        plays: [low, high],
        scores,
        captured,
        restored: deck.cards().iter().copied().collect(),
    })
}

fn alloy_projection(receipt: &Value) -> Result<RoundProjection, AlloyConformanceError> {
    let command = receipt
        .pointer("/commands/CanonicalOneCardRustFixture")
        .and_then(Value::as_object)
        .ok_or_else(|| problem("Alloy receipt omitted canonical command object"))?;
    let values = command
        .get("solution")
        .and_then(Value::as_array)
        .and_then(|solutions| solutions.first())
        .and_then(|solution| solution.get("instances"))
        .and_then(Value::as_array)
        .and_then(|instances| instances.first())
        .and_then(|instance| instance.get("values"))
        .and_then(Value::as_object)
        .ok_or_else(|| problem("Alloy canonical solution omitted instance values"))?;
    let roles = object_by_suffix(values, "CanonicalRoles$0")?;
    let dealer_atom = unary_atom(roles, "dealerRole")?;
    let first_atom = unary_atom(roles, "firstRole")?;
    let low_atom = unary_atom(roles, "lowLead")?;
    let high_atom = unary_atom(roles, "highFollow")?;
    let trump_atom = unary_atom(roles, "trumpRole")?;
    let round_atom = unary_atom(roles, "roundRole")?;
    let trick_atom = unary_atom(roles, "trickRole")?;
    let round = object_by_exact_atom(values, round_atom)?;
    let trick = object_by_exact_atom(values, trick_atom)?;
    let player_index = |atom: &str| -> Result<u8, AlloyConformanceError> {
        if atom == first_atom {
            Ok(0)
        } else if atom == dealer_atom {
            Ok(1)
        } else {
            Err(problem(format!(
                "unlabelled player atom in projection: {atom}"
            )))
        }
    };
    let cards = card_map(values)?;
    let card = |atom: &str| {
        cards
            .get(atom)
            .copied()
            .ok_or_else(|| problem(format!("unknown Alloy card atom: {atom}")))
    };

    let mut hands: [BTreeSet<Card>; 2] = std::array::from_fn(|_| BTreeSet::new());
    for row in relation_rows(round, "hand", 2)? {
        hands[usize::from(player_index(&row[0])?)].insert(card(&row[1])?);
    }
    let mut bids = [None; 2];
    for row in relation_rows(round, "bid", 2)? {
        bids[usize::from(player_index(&row[0])?)] = Some(parse_u8(&row[1], "bid")?);
    }
    let mut plays = [None; 2];
    for row in relation_rows(trick, "play", 2)? {
        plays[usize::from(player_index(&row[0])?)] = Some(card(&row[1])?);
    }
    let mut scores = [None; 2];
    for row in relation_rows(round, "score", 2)? {
        scores[usize::from(player_index(&row[0])?)] = Some(parse_u16(&row[1], "score")?);
    }
    let mut captured: [BTreeSet<Card>; 2] = std::array::from_fn(|_| BTreeSet::new());
    for row in relation_rows(round, "captured", 2)? {
        captured[usize::from(player_index(&row[0])?)].insert(card(&row[1])?);
    }
    let restored = relation_rows(round, "restored", 1)?
        .into_iter()
        .map(|row| card(&row[0]))
        .collect::<Result<BTreeSet<_>, _>>()?;

    Ok(RoundProjection {
        dealer: player_index(dealer_atom)?,
        first: player_index(first_atom)?,
        low_lead: card(low_atom)?,
        high_follow: card(high_atom)?,
        trump: card(trump_atom)?,
        hands,
        bids: require_pair(bids, "bids")?,
        leader: player_index(unary_atom(trick, "leader")?)?,
        winner: player_index(unary_atom(trick, "winner")?)?,
        plays: require_pair(plays, "plays")?,
        scores: require_pair(scores, "scores")?,
        captured,
        restored,
    })
}

fn compare_projection(
    rust: &RoundProjection,
    alloy: &RoundProjection,
) -> Result<(), AlloyConformanceError> {
    if rust == alloy {
        Ok(())
    } else {
        Err(problem(format!(
            "canonical relational projection differs\nRust: {rust:#?}\nAlloy: {alloy:#?}"
        )))
    }
}

fn discriminate_rust_structures_and_defects() -> Result<(), AlloyConformanceError> {
    let mut duplicate = Card::standard_deck();
    duplicate[1] = duplicate[0];
    if !matches!(
        DeckOrder::new(duplicate),
        Err(RuleViolation::DuplicateCard(_))
    ) {
        return Err(problem(
            "Rust accepted the duplicate-card structural defect",
        ));
    }

    let exact_partial = score_round(1, 1, 2);
    if exact_partial.outcome != BidOutcome::Exact || exact_partial.points != 11 {
        return Err(problem(
            "Rust admitted the partial all-tricks scoring defect",
        ));
    }
    let lead = seat(0)?;
    let follower = seat(1)?;
    let correct = trick_winner(
        Suit::Spades,
        &[
            PlayedCard {
                player: lead,
                card: Card::new(Suit::Hearts, Rank::Two),
            },
            PlayedCard {
                player: follower,
                card: Card::new(Suit::Clubs, Rank::Ace),
            },
        ],
    );
    if correct != Some(lead) {
        return Err(problem("Rust admitted the rank-only trick-winner defect"));
    }
    reject_off_suit_while_holding_lead()?;
    Ok(())
}

fn reject_off_suit_while_holding_lead() -> Result<(), AlloyConformanceError> {
    let dealer = seat(1)?;
    let first = seat(0)?;
    let low = Card::new(Suit::Clubs, Rank::Two);
    let high = Card::new(Suit::Clubs, Rank::Three);
    let mut game = Game::<2>::new(dealer).map_err(rust_error("construct follow-suit game"))?;
    game = apply(&game, Action::Deal(DeckOrder::standard()), "first deal")?;
    game = apply(
        &game,
        Action::Bid {
            player: first,
            tricks: 0,
        },
        "first bid",
    )?;
    game = apply(
        &game,
        Action::Bid {
            player: dealer,
            tricks: 1,
        },
        "second bid",
    )?;
    game = apply(
        &game,
        Action::Play {
            player: first,
            card: low,
        },
        "first lead",
    )?;
    game = apply(
        &game,
        Action::Play {
            player: dealer,
            card: high,
        },
        "first follow",
    )?;
    game = apply(&game, Action::SettleRound, "first settlement")?;

    let hearts_two = Card::new(Suit::Hearts, Rank::Two);
    let hearts_three = Card::new(Suit::Hearts, Rank::Three);
    let clubs_two = Card::new(Suit::Clubs, Rank::Two);
    let clubs_three = Card::new(Suit::Clubs, Rank::Three);
    let spades_two = Card::new(Suit::Spades, Rank::Two);
    let deck = deck_with_prefix([hearts_two, hearts_three, clubs_three, clubs_two, spades_two])?;
    game = apply(&game, Action::Deal(deck), "second deal")?;
    let second_first = seat(1)?;
    let second_follower = seat(0)?;
    game = apply(
        &game,
        Action::Bid {
            player: second_first,
            tricks: 0,
        },
        "second-round first bid",
    )?;
    game = apply(
        &game,
        Action::Bid {
            player: second_follower,
            tricks: 0,
        },
        "second-round second bid",
    )?;
    game = apply(
        &game,
        Action::Play {
            player: second_first,
            card: hearts_two,
        },
        "second-round lead",
    )?;
    let rejected = game.transition(Action::Play {
        player: second_follower,
        card: clubs_two,
    });
    if matches!(rejected, Err(RuleViolation::MustFollowSuit(Suit::Hearts))) {
        Ok(())
    } else {
        Err(problem(format!(
            "Rust did not reject the controlled follow-suit defect: {rejected:?}"
        )))
    }
}

fn deck_with_prefix<const N: usize>(prefix: [Card; N]) -> Result<DeckOrder, AlloyConformanceError> {
    let prefix_set = prefix.into_iter().collect::<BTreeSet<_>>();
    if prefix_set.len() != N {
        return Err(problem("custom deck prefix contains duplicates"));
    }
    let mut cards = prefix.to_vec();
    cards.extend(
        Card::standard_deck()
            .into_iter()
            .filter(|card| !prefix_set.contains(card)),
    );
    let cards: [Card; DECK_SIZE] = cards
        .try_into()
        .map_err(|cards: Vec<Card>| problem(format!("custom deck has {} cards", cards.len())))?;
    DeckOrder::new(cards).map_err(rust_error("validate custom follow-suit deck"))
}

fn card_map(values: &Map<String, Value>) -> Result<BTreeMap<String, Card>, AlloyConformanceError> {
    let mut cards = BTreeMap::new();
    for (atom, value) in values {
        if !atom.contains("/Card$") {
            continue;
        }
        let object = value
            .as_object()
            .ok_or_else(|| problem(format!("Alloy card atom {atom} is not an object")))?;
        let suit = match atom_label(unary_atom(object, "suit")?) {
            "Clubs" => Suit::Clubs,
            "Diamonds" => Suit::Diamonds,
            "Hearts" => Suit::Hearts,
            "Spades" => Suit::Spades,
            other => return Err(problem(format!("unknown Alloy suit atom: {other}"))),
        };
        let rank = match atom_label(unary_atom(object, "rank")?) {
            "Two" => Rank::Two,
            "Three" => Rank::Three,
            "Four" => Rank::Four,
            "Five" => Rank::Five,
            "Six" => Rank::Six,
            "Seven" => Rank::Seven,
            "Eight" => Rank::Eight,
            "Nine" => Rank::Nine,
            "Ten" => Rank::Ten,
            "Jack" => Rank::Jack,
            "Queen" => Rank::Queen,
            "King" => Rank::King,
            "Ace" => Rank::Ace,
            other => return Err(problem(format!("unknown Alloy rank atom: {other}"))),
        };
        if cards.insert(atom.clone(), Card::new(suit, rank)).is_some() {
            return Err(problem(format!("duplicate Alloy card atom: {atom}")));
        }
    }
    if cards.len() != DECK_SIZE {
        return Err(problem(format!(
            "canonical Alloy instance has {} mapped cards, expected {DECK_SIZE}",
            cards.len()
        )));
    }
    if cards.values().copied().collect::<BTreeSet<_>>().len() != DECK_SIZE {
        return Err(problem("canonical Alloy card atoms do not map bijectively"));
    }
    Ok(cards)
}

fn object_by_suffix<'a>(
    values: &'a Map<String, Value>,
    suffix: &str,
) -> Result<&'a Map<String, Value>, AlloyConformanceError> {
    let matches = values
        .iter()
        .filter(|(name, _)| name.ends_with(suffix))
        .collect::<Vec<_>>();
    if let [(name, value)] = matches.as_slice() {
        value
            .as_object()
            .ok_or_else(|| problem(format!("Alloy value {name} is not an object")))
    } else {
        Err(problem(format!(
            "expected one Alloy value ending in {suffix:?}, found {}",
            matches.len()
        )))
    }
}

fn object_by_exact_atom<'a>(
    values: &'a Map<String, Value>,
    atom: &str,
) -> Result<&'a Map<String, Value>, AlloyConformanceError> {
    values
        .get(atom)
        .and_then(Value::as_object)
        .ok_or_else(|| problem(format!("Alloy receipt omitted atom object {atom}")))
}

fn relation_rows(
    object: &Map<String, Value>,
    field: &str,
    arity: usize,
) -> Result<Vec<Vec<String>>, AlloyConformanceError> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| problem(format!("Alloy object omitted relation {field}")))?
        .iter()
        .map(|row| {
            let row = row
                .as_array()
                .ok_or_else(|| problem(format!("Alloy relation {field} has a non-row value")))?;
            if row.len() != arity {
                return Err(problem(format!(
                    "Alloy relation {field} row has arity {}, expected {arity}",
                    row.len()
                )));
            }
            row.iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        problem(format!("Alloy relation {field} has non-string atom"))
                    })
                })
                .collect()
        })
        .collect()
}

fn unary_atom<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, AlloyConformanceError> {
    let rows = object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| problem(format!("Alloy object omitted unary relation {field}")))?;
    if let [row] = rows.as_slice()
        && let Some(values) = row.as_array()
        && let [value] = values.as_slice()
        && let Some(atom) = value.as_str()
    {
        return Ok(atom);
    }
    Err(problem(format!(
        "Alloy relation {field} is not exactly one unary tuple"
    )))
}

fn require_pair<T: Copy>(
    pair: [Option<T>; 2],
    label: &str,
) -> Result<[T; 2], AlloyConformanceError> {
    match pair {
        [Some(first), Some(second)] => Ok([first, second]),
        _ => Err(problem(format!("Alloy projection has incomplete {label}"))),
    }
}

fn parse_u8(value: &str, label: &str) -> Result<u8, AlloyConformanceError> {
    value
        .parse()
        .map_err(|error| problem(format!("invalid Alloy {label} integer {value:?}: {error}")))
}

fn parse_u16(value: &str, label: &str) -> Result<u16, AlloyConformanceError> {
    value
        .parse()
        .map_err(|error| problem(format!("invalid Alloy {label} integer {value:?}: {error}")))
}

fn atom_label(atom: &str) -> &str {
    atom.rsplit('/')
        .next()
        .unwrap_or(atom)
        .split('$')
        .next()
        .unwrap_or(atom)
}

fn seat(index: usize) -> Result<Seat<2>, AlloyConformanceError> {
    Seat::new(index).map_err(rust_error("construct two-player seat"))
}

fn seat_number(seat: Seat<2>) -> Result<u8, AlloyConformanceError> {
    u8::try_from(seat.index())
        .map_err(|error| problem(format!("seat index does not fit u8: {error}")))
}

fn apply(
    game: &Game<2>,
    action: Action<2>,
    context: &'static str,
) -> Result<Game<2>, AlloyConformanceError> {
    game.transition(action)
        .map(|transition| transition.next)
        .map_err(rust_error(context))
}

fn rust_error(context: &'static str) -> impl FnOnce(RuleViolation) -> AlloyConformanceError {
    move |error| problem(format!("{context}: {error:?}"))
}

fn problem(message: impl Into<String>) -> AlloyConformanceError {
    AlloyConformanceError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloy_structural_and_bounded_agreement() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = compare_rust_alloy(&root).expect("Rust and Alloy should agree");
        assert_eq!(report.command_count, 10);
        assert_eq!(report.valid_instances, 1);
        assert_eq!(report.invalid_structures_rejected, 5);
        assert_eq!(report.assertions_checked, 1);
        assert_eq!(report.controlled_defect_witnesses, 3);
        assert_eq!(report.projection_groups_compared, 13);
        assert_eq!(report.command_scopes.len(), 10);
    }

    #[test]
    fn alloy_atom_labels_ignore_module_and_instance_suffixes() {
        assert_eq!(atom_label("poche/Clubs$0"), "Clubs");
        assert_eq!(atom_label("Ace$0"), "Ace");
    }
}

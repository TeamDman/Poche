// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use poche_model::{PropertyId, property_catalog};
use poche_native_tools::{NativeDisposition, run_prolog_fixture};
use poche_oracle_rust::{
    Action, BidOutcome, Card, DeckOrder, Game, GameState, PlayedCard, Rank, RoundScore, Seat, Suit,
    Turn, score_round, trick_winner,
};

/// One tracked cross-model fixture manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologFixtureSpec {
    /// Shared/corpus fixture ID.
    pub id: String,
    /// Ground native goal atom.
    pub native_goal: String,
    /// Rule IDs that justify the comparison.
    pub rules: Vec<String>,
    /// Productive-mode and scope qualification.
    pub mode: String,
    /// Source manifest path.
    pub path: PathBuf,
}

/// Successful exact comparison for one fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologFixtureComparison {
    /// Fixture ID.
    pub id: String,
    /// Number of normalized set rows compared.
    pub answer_count: usize,
    /// Rule origins from the fixture manifest.
    pub rules: Vec<String>,
    /// Explicit mode/scope qualification.
    pub mode: String,
}

/// Complete Rust-versus-Scryer answer-set report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologConformanceReport {
    /// All fixture comparisons in lexical manifest order.
    pub fixtures: Vec<PrologFixtureComparison>,
    /// Total normalized rows compared.
    pub answer_count: usize,
    /// Number of rule/explanation relation rows.
    pub explanation_count: usize,
    /// Intentional semantic adapter boundary.
    pub collect_boundary: &'static str,
    /// Productive reverse-search boundary.
    pub predecessor_boundary: &'static str,
}

/// Fixture, native execution, or semantic mismatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologConformanceError(String);

impl fmt::Display for PrologConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PrologConformanceError {}

/// Execute every tracked Prolog fixture and compare its answer set with the
/// independently authored conventional Rust oracle/property catalog.
///
/// # Errors
///
/// Returns a diagnostic when manifests are malformed, native execution is not
/// a typed success, a native row is malformed, or either normalized set has a
/// missing/extra answer.
pub fn compare_rust_prolog(
    root: &Path,
    fixture_directory: &Path,
) -> Result<PrologConformanceReport, PrologConformanceError> {
    let fixtures = load_fixtures(fixture_directory)?;
    let mut comparisons = Vec::with_capacity(fixtures.len());
    let mut total = 0;
    let mut explanation_count = 0;
    for fixture in fixtures {
        let native = run_prolog_fixture(root, &fixture.id, &fixture.native_goal);
        if native.disposition != NativeDisposition::Success {
            return Err(problem(format!(
                "{} native query was {:?}: {} ({})",
                fixture.id,
                native.disposition,
                native.diagnostic,
                native.evidence_directory.display()
            )));
        }
        let actual = normalize_native_answers(&fixture.native_goal, &native.answers)?;
        let expected = rust_answers(&fixture.native_goal)?;
        compare_sets(&fixture.id, &expected, &actual)?;
        if fixture.native_goal == "rule_explanations" {
            explanation_count = native.answers.len();
            verify_explanation_rules(&fixture, &actual)?;
        }
        total += actual.len();
        comparisons.push(PrologFixtureComparison {
            id: fixture.id,
            answer_count: actual.len(),
            rules: fixture.rules,
            mode: fixture.mode,
        });
    }
    Ok(PrologConformanceReport {
        fixtures: comparisons,
        answer_count: total,
        explanation_count,
        collect_boundary: "Prolog play-to-collect plus collect-to-settle is compared with Rust's single complete-trick transition.",
        predecessor_boundary: "Previous and Action are recovered only from three ground bounded successors; once/1 commits the deterministic first proof and prevents unproductive alternative term search.",
    })
}

fn verify_explanation_rules(
    fixture: &PrologFixtureSpec,
    actual: &BTreeSet<String>,
) -> Result<(), PrologConformanceError> {
    let actual_rules = actual
        .iter()
        .filter_map(|row| parse_rule_identity(row).map(|(_, rule)| rule.to_owned()))
        .collect::<BTreeSet<_>>();
    let declared = fixture.rules.iter().cloned().collect::<BTreeSet<_>>();
    if actual_rules == declared {
        Ok(())
    } else {
        Err(problem(format!(
            "{} manifest rule set differs from native explanations",
            fixture.id
        )))
    }
}

fn load_fixtures(directory: &Path) -> Result<Vec<PrologFixtureSpec>, PrologConformanceError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        problem(format!(
            "could not read fixture directory {}: {error}",
            directory.display()
        ))
    })?;
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "fixture")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut fixtures = paths
        .into_iter()
        .map(|path| parse_fixture(&path))
        .collect::<Result<Vec<_>, _>>()?;
    fixtures.sort_by(|left, right| left.id.cmp(&right.id));
    let expected = BTreeSet::from([
        "canonical-round-successors",
        "legal-actions-from-state",
        "predecessors-for-action-and-state",
        "rule-explanations",
        "score-causes",
        "trick-winner-relations",
    ]);
    let actual = fixtures
        .iter()
        .map(|fixture| fixture.id.as_str())
        .collect::<BTreeSet<_>>();
    if actual == expected {
        Ok(fixtures)
    } else {
        Err(problem(format!(
            "fixture corpus IDs differ: expected={expected:?}, actual={actual:?}"
        )))
    }
}

fn parse_fixture(path: &Path) -> Result<PrologFixtureSpec, PrologConformanceError> {
    let source = fs::read_to_string(path)
        .map_err(|error| problem(format!("could not read {}: {error}", path.display())))?;
    let mut schema = None;
    let mut id = None;
    let mut native_goal = None;
    let mut rules = None;
    let mut mode = None;
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once('=') else {
            return Err(problem(format!(
                "{} has malformed line `{line}`",
                path.display()
            )));
        };
        match key.trim() {
            "schema_version" => schema = value.trim().parse::<u8>().ok(),
            "id" => id = quoted(value.trim()),
            "native_goal" => native_goal = quoted(value.trim()),
            "rules" => rules = Some(quoted_array(value.trim())?),
            "mode" => mode = quoted(value.trim()),
            other => {
                return Err(problem(format!(
                    "{} has unknown fixture field `{other}`",
                    path.display()
                )));
            }
        }
    }
    if schema != Some(1) {
        return Err(problem(format!(
            "{} requires schema_version = 1",
            path.display()
        )));
    }
    let spec = PrologFixtureSpec {
        id: id.ok_or_else(|| problem(format!("{} has no quoted id", path.display())))?,
        native_goal: native_goal
            .ok_or_else(|| problem(format!("{} has no quoted native_goal", path.display())))?,
        rules: rules.ok_or_else(|| problem(format!("{} has no rules", path.display())))?,
        mode: mode.ok_or_else(|| problem(format!("{} has no quoted mode", path.display())))?,
        path: path.to_owned(),
    };
    if spec.rules.is_empty() || spec.mode.is_empty() {
        Err(problem(format!(
            "{} has an empty rule/mode contract",
            path.display()
        )))
    } else {
        Ok(spec)
    }
}

fn quoted(value: &str) -> Option<String> {
    value
        .strip_prefix('"')?
        .strip_suffix('"')
        .map(str::to_owned)
}

fn quoted_array(value: &str) -> Result<Vec<String>, PrologConformanceError> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| problem(format!("malformed quoted array `{value}`")))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|item| {
            quoted(item.trim()).ok_or_else(|| problem(format!("malformed quoted item `{item}`")))
        })
        .collect()
}

fn normalize_native_answers(
    native_goal: &str,
    answers: &BTreeSet<String>,
) -> Result<BTreeSet<String>, PrologConformanceError> {
    if native_goal != "rule_explanations" {
        return Ok(answers.clone());
    }
    answers
        .iter()
        .map(|row| {
            let inner = row
                .strip_prefix("rule(")
                .and_then(|value| value.strip_suffix(')'))
                .ok_or_else(|| problem(format!("malformed rule explanation row `{row}`")))?;
            let (identity, explanation) = inner
                .rsplit_once(',')
                .ok_or_else(|| problem(format!("rule explanation has no explanation: `{row}`")))?;
            if explanation.is_empty() {
                Err(problem(format!("rule explanation is empty: `{row}`")))
            } else {
                Ok(format!("rule({identity})"))
            }
        })
        .collect()
}

fn rust_answers(native_goal: &str) -> Result<BTreeSet<String>, PrologConformanceError> {
    match native_goal {
        "legal_actions" => legal_action_answers(),
        "successors" => successor_answers(),
        "predecessors" => predecessor_answers(),
        "trick_winners" => trick_winner_answers(),
        "round_scoring" => Ok(scoring_answers()),
        "rule_explanations" => rule_identity_answers(),
        other => Err(problem(format!("no Rust fixture adapter for `{other}`"))),
    }
}

fn canonical_games() -> Result<BTreeMap<String, Game<2>>, PrologConformanceError> {
    let seat0 = seat(0)?;
    let seat1 = seat(1)?;
    let mut games = BTreeMap::new();
    let initial = Game::new(seat1).map_err(debug_problem("construct initial game"))?;
    games.insert("initial".to_owned(), initial.clone());
    let after_deal = apply(&initial, Action::Deal(DeckOrder::standard()))?;
    games.insert("after_deal".to_owned(), after_deal.clone());
    for bid0 in 0..=1 {
        let single_bid_game = apply(
            &after_deal,
            Action::Bid {
                player: seat0,
                tricks: bid0,
            },
        )?;
        games.insert(format!("after_bid0({bid0})"), single_bid_game.clone());
        for bid1 in 0..=1 {
            let both_bids_game = apply(
                &single_bid_game,
                Action::Bid {
                    player: seat1,
                    tricks: bid1,
                },
            )?;
            games.insert(format!("after_bids({bid0},{bid1})"), both_bids_game.clone());
            let lead = only_legal_action(&both_bids_game)?;
            let after_lead = apply(&both_bids_game, lead)?;
            games.insert(format!("after_lead({bid0},{bid1})"), after_lead.clone());
            let follow = only_legal_action(&after_lead)?;
            let after_trick = apply(&after_lead, follow)?;
            games.insert(format!("after_trick({bid0},{bid1})"), after_trick);
        }
    }
    Ok(games)
}

fn legal_action_answers() -> Result<BTreeSet<String>, PrologConformanceError> {
    let games = canonical_games()?;
    let mut rows = BTreeSet::new();
    for (name, game) in &games {
        match game.turn() {
            Turn::Chance if name == "initial" => {
                rows.insert("legal(initial,deal)".to_owned());
            }
            Turn::Player(_) => {
                for action in game.legal_player_actions() {
                    rows.insert(format!("legal({name},{})", render_action(&action)));
                }
            }
            Turn::Environment if name.starts_with("after_trick") => {
                rows.insert(format!("legal({name},settle)"));
            }
            Turn::Chance | Turn::Environment | Turn::Finished => {}
        }
    }
    Ok(rows)
}

fn successor_answers() -> Result<BTreeSet<String>, PrologConformanceError> {
    let games = canonical_games()?;
    let mut rows = BTreeSet::new();
    for (name, game) in &games {
        let actions = match game.turn() {
            Turn::Chance if name == "initial" => vec![Action::Deal(DeckOrder::standard())],
            Turn::Player(_) if !name.starts_with("after_trick") => game.legal_player_actions(),
            Turn::Chance | Turn::Player(_) | Turn::Environment | Turn::Finished => Vec::new(),
        };
        for action in actions {
            let next = apply(game, action.clone())?;
            rows.insert(format!(
                "successor({name},{},{})",
                render_action(&action),
                render_state(&next)?
            ));
        }
    }
    Ok(rows)
}

fn predecessor_answers() -> Result<BTreeSet<String>, PrologConformanceError> {
    let games = canonical_games()?;
    let cases = [
        (
            "after_bid0(0)",
            "after_deal",
            Action::Bid {
                player: seat(0)?,
                tricks: 0,
            },
        ),
        (
            "after_bids(0,1)",
            "after_bid0(0)",
            Action::Bid {
                player: seat(1)?,
                tricks: 1,
            },
        ),
        (
            "after_lead(0,1)",
            "after_bids(0,1)",
            Action::Play {
                player: seat(0)?,
                card: Card::new(Suit::Clubs, Rank::Two),
            },
        ),
    ];
    let mut rows = BTreeSet::new();
    for (target_name, previous_name, action) in cases {
        let previous = games
            .get(previous_name)
            .ok_or_else(|| problem(format!("missing Rust state {previous_name}")))?;
        let target = games
            .get(target_name)
            .ok_or_else(|| problem(format!("missing Rust state {target_name}")))?;
        if &apply(previous, action.clone())? != target {
            return Err(problem(format!(
                "Rust predecessor replay failed for {target_name}"
            )));
        }
        rows.insert(format!(
            "predecessor({target_name},{},{})",
            render_action(&action),
            render_state(previous)?
        ));
    }
    Ok(rows)
}

fn trick_winner_answers() -> Result<BTreeSet<String>, PrologConformanceError> {
    let cases = [
        (
            "trump_low",
            Suit::Spades,
            [(0, Suit::Hearts, Rank::Ace), (1, Suit::Spades, Rank::Two)],
        ),
        (
            "lead_over_off_suit",
            Suit::Spades,
            [(0, Suit::Hearts, Rank::Ten), (1, Suit::Clubs, Rank::Ace)],
        ),
        (
            "higher_lead",
            Suit::Spades,
            [(0, Suit::Hearts, Rank::Ten), (1, Suit::Hearts, Rank::Jack)],
        ),
        (
            "off_suit_ineligible",
            Suit::Spades,
            [(0, Suit::Hearts, Rank::Two), (1, Suit::Clubs, Rank::Ace)],
        ),
    ];
    let mut rows = BTreeSet::new();
    for (name, trump, inputs) in cases {
        let plays = inputs
            .into_iter()
            .map(|(player, suit, rank)| {
                Ok(PlayedCard {
                    player: seat(player)?,
                    card: Card::new(suit, rank),
                })
            })
            .collect::<Result<Vec<_>, PrologConformanceError>>()?;
        let winner = trick_winner(trump, &plays)
            .ok_or_else(|| problem(format!("Rust had no winner for {name}")))?;
        rows.insert(format!("winner({name},{})", winner.index()));
    }
    Ok(rows)
}

fn scoring_answers() -> BTreeSet<String> {
    let mut rows = BTreeSet::new();
    for hand_size in 1..=7 {
        for bid in 0..=hand_size {
            for tricks in 0..=hand_size {
                let score = score_round(bid, tricks, hand_size);
                rows.insert(format!(
                    "score({hand_size},{bid},{tricks},{},{},{})",
                    score.points,
                    render_score_cell(score),
                    score.payment_cents / 10
                ));
            }
        }
    }
    rows
}

fn rule_identity_answers() -> Result<BTreeSet<String>, PrologConformanceError> {
    let catalog = property_catalog();
    let mappings = [
        (PropertyId::LegalActor, &["legal_action"][..]),
        (PropertyId::PhaseProgress, &["step", "predecessor"][..]),
        (PropertyId::TrickWinner, &["trick_winner"][..]),
        (PropertyId::ScoringAndPot, &["round_score"][..]),
    ];
    let mut rows = BTreeSet::new();
    for (property_id, relations) in mappings {
        let property = catalog
            .iter()
            .find(|property| property.id == property_id)
            .ok_or_else(|| problem(format!("property catalog omitted {property_id:?}")))?;
        if property.explanation.is_empty() {
            return Err(problem(format!("{property_id:?} has no Rust explanation")));
        }
        for relation in relations {
            for rule in &property.rules {
                rows.insert(format!("rule({relation},{})", rule.rule_id));
            }
        }
    }
    Ok(rows)
}

fn render_state(game: &Game<2>) -> Result<String, PrologConformanceError> {
    match game.state() {
        GameState::AwaitingDeal(state) => Ok(format!(
            "round_state(deal,{},1,{},[[],[]],none,[none,none],{},[],[[],[]],[0,0],{},{})",
            state.ledger.dealer.index(),
            render_cards(Card::standard_deck()),
            state.ledger.dealer.left().index(),
            render_numbers(state.ledger.scores),
            state.ledger.pot_cents
        )),
        GameState::Bidding(state) => Ok(format!(
            "round_state(bid({}),{},{},{},{},{},{},{},[],[[],[]],[0,0],{},{})",
            state.actor.index(),
            state.ledger.dealer.index(),
            state.hand_size,
            render_cards(state.stock.iter()),
            render_hands(&state.hands),
            render_card(state.trump),
            render_bids(state.bids),
            state.ledger.dealer.left().index(),
            render_numbers(state.ledger.scores),
            state.ledger.pot_cents
        )),
        GameState::Playing(state) => Ok(format!(
            "round_state(play({}),{},{},{},{},{},{},{},{},{},{},{},{})",
            state.actor.index(),
            state.ledger.dealer.index(),
            state.hand_size,
            render_cards(state.stock.iter()),
            render_hands(&state.hands),
            render_card(state.trump),
            render_numbers(state.bids),
            state.leader.index(),
            render_plays(state.trick.iter()),
            render_piles(&state.captured),
            render_numbers(state.tricks_won),
            render_numbers(state.ledger.scores),
            state.ledger.pot_cents
        )),
        GameState::Scoring(state) => {
            let leader = state
                .tricks_won
                .iter()
                .position(|won| *won == 1)
                .ok_or_else(|| problem("one-card scoring state has no winner"))?;
            Ok(format!(
                "round_state(settle,{},{},{},[[],[]],{},{},{},[],{},{},{},{})",
                state.ledger.dealer.index(),
                state.hand_size,
                render_cards(state.stock.iter()),
                render_card(state.trump),
                render_numbers(state.bids),
                leader,
                render_piles(&state.captured),
                render_numbers(state.tricks_won),
                render_numbers(state.ledger.scores),
                state.ledger.pot_cents
            ))
        }
        GameState::Finished(_) => Err(problem(
            "the one-round Prolog state corpus has no full-game Finished term",
        )),
    }
}

fn render_action(action: &Action<2>) -> String {
    match action {
        Action::Deal(_) => "deal".to_owned(),
        Action::Bid { player, tricks } => format!("bid({},{tricks})", player.index()),
        Action::Play { player, card } => {
            format!("play({},{})", player.index(), render_card(*card))
        }
        Action::SettleRound => "settle".to_owned(),
    }
}

fn render_card(card: Card) -> String {
    format!(
        "card({},{})",
        render_suit(card.suit),
        render_rank(card.rank)
    )
}

const fn render_suit(suit: Suit) -> &'static str {
    match suit {
        Suit::Clubs => "clubs",
        Suit::Diamonds => "diamonds",
        Suit::Hearts => "hearts",
        Suit::Spades => "spades",
    }
}

const fn render_rank(rank: Rank) -> u8 {
    match rank {
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Ace => 14,
    }
}

fn render_cards(cards: impl IntoIterator<Item = Card>) -> String {
    render_list(cards.into_iter().map(render_card))
}

fn render_hands(hands: &[poche_oracle_rust::Hand; 2]) -> String {
    render_list(hands.iter().map(|hand| render_cards(hand.iter())))
}

fn render_piles(piles: &[poche_oracle_rust::CardPile; 2]) -> String {
    render_list(piles.iter().map(|pile| render_cards(pile.iter())))
}

fn render_plays(plays: impl IntoIterator<Item = PlayedCard<2>>) -> String {
    render_list(
        plays
            .into_iter()
            .map(|play| format!("play({},{})", play.player.index(), render_card(play.card))),
    )
}

fn render_bids(bids: [Option<u8>; 2]) -> String {
    render_list(
        bids.into_iter()
            .map(|bid| bid.map_or_else(|| "none".to_owned(), |bid| bid.to_string())),
    )
}

fn render_numbers<T: fmt::Display>(values: impl IntoIterator<Item = T>) -> String {
    render_list(values.into_iter().map(|value| value.to_string()))
}

fn render_list(values: impl IntoIterator<Item = String>) -> String {
    let mut output = String::from("[");
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push_str(&value);
    }
    output.push(']');
    output
}

fn render_score_cell(score: RoundScore) -> String {
    match score.outcome {
        BidOutcome::Missed => "poche".to_owned(),
        BidOutcome::Exact => format!("exact({})", score.bid),
        BidOutcome::AllTricks => format!("all_tricks({})", score.bid),
    }
}

fn apply(game: &Game<2>, action: Action<2>) -> Result<Game<2>, PrologConformanceError> {
    game.transition(action)
        .map(|transition| transition.next)
        .map_err(debug_problem("apply Rust action"))
}

fn only_legal_action(game: &Game<2>) -> Result<Action<2>, PrologConformanceError> {
    let actions = game.legal_player_actions();
    match actions.as_slice() {
        [action] => Ok(action.clone()),
        _ => Err(problem(format!(
            "expected one canonical legal card action, found {}",
            actions.len()
        ))),
    }
}

fn seat(index: usize) -> Result<Seat<2>, PrologConformanceError> {
    Seat::new(index).map_err(debug_problem("construct two-player seat"))
}

fn compare_sets(
    fixture_id: &str,
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), PrologConformanceError> {
    if expected == actual {
        return Ok(());
    }
    let missing = expected.difference(actual).take(3).collect::<Vec<_>>();
    let extra = actual.difference(expected).take(3).collect::<Vec<_>>();
    Err(problem(format!(
        "{fixture_id} answer-set mismatch: expected={}, actual={}, missing={missing:?}, extra={extra:?}",
        expected.len(),
        actual.len()
    )))
}

fn parse_rule_identity(row: &str) -> Option<(&str, &str)> {
    let inner = row.strip_prefix("rule(")?.strip_suffix(')')?;
    inner.split_once(',')
}

fn debug_problem<E: fmt::Debug>(context: &'static str) -> impl FnOnce(E) -> PrologConformanceError {
    move |error| problem(format!("{context}: {error:?}"))
}

fn problem(detail: impl Into<String>) -> PrologConformanceError {
    PrologConformanceError(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prolog_native_answer_sets_match_independent_rust() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixtures = root.join("tests/fixtures/prolog");
        let report = compare_rust_prolog(&root, &fixtures).expect("all answer sets agree");
        assert_eq!(report.fixtures.len(), 6);
        assert_eq!(report.answer_count, 264);
        assert_eq!(report.explanation_count, 20);
        assert!(
            report
                .fixtures
                .iter()
                .all(|fixture| !fixture.rules.is_empty())
        );
        assert!(
            report
                .fixtures
                .iter()
                .all(|fixture| !fixture.mode.is_empty())
        );
    }

    #[test]
    fn prolog_fixture_manifests_are_strict() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixtures =
            load_fixtures(&root.join("tests/fixtures/prolog")).expect("tracked fixtures parse");
        assert_eq!(fixtures.len(), 6);
        assert!(fixtures.iter().all(|fixture| fixture.path.is_file()));
    }
}

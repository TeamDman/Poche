use proptest::arbitrary::{Arbitrary, any};
use proptest::prelude::{BoxedStrategy, Strategy};
use proptest::{prop_assert, prop_assert_eq, proptest};

use poche_oracle_rust as oracle;

use crate::{
    Bid, ChanceAction, Game, LegalActions, ModelAction, Phase, Player, PlayerAction, RoundOutcome,
    Transition, evaluate_transition_property, property_catalog,
};

const REGRESSION_SEEDS: [u64; 6] = [
    0,
    1,
    20_260_803,
    0x5eed_fade_cafe_beef,
    0x9e37_79b9_7f4a_7c15,
    u64::MAX,
];

#[derive(Clone, Copy, Debug)]
struct TraceSeed(u64);

impl Arbitrary for TraceSeed {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        any::<u64>().prop_map(Self).boxed()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SampledTable {
    Two,
    Three,
    Seven,
    FiftyOne,
}

impl SampledTable {
    const fn players(self) -> usize {
        match self {
            Self::Two => 2,
            Self::Three => 3,
            Self::Seven => 7,
            Self::FiftyOne => 51,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LargerScopeCase {
    seed: u64,
    table: SampledTable,
}

impl Arbitrary for LargerScopeCase {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<u64>(),
            proptest::sample::select(vec![
                SampledTable::Two,
                SampledTable::Three,
                SampledTable::Seven,
                SampledTable::FiftyOne,
            ]),
        )
            .prop_map(|(seed, table)| Self { seed, table })
            .boxed()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MicroSummary {
    actions: Vec<ModelAction>,
    phases: Vec<Phase>,
    scores: [u8; 2],
    pot_cents: u16,
    winners: [bool; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FullDeckSummary {
    confidence: &'static str,
    seed: u64,
    players: usize,
    steps: usize,
    scores: Vec<u16>,
    pot_cents: u32,
    winners: Vec<bool>,
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, len: usize) -> usize {
        let modulus = u64::try_from(len).expect("finite action/deck length fits u64");
        usize::try_from(self.next() % modulus).expect("selected index fits usize")
    }
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(16))]

    #[test]
    fn proptest_transition_equivalence(seed in any::<TraceSeed>()) {
        let first = run_micro(seed.0);
        prop_assert!(first.is_ok(), "seed {} failed: {:?}", seed.0, first);
        let first = first.unwrap();
        let replay = run_micro(seed.0).unwrap();
        prop_assert_eq!(first, replay, "seed {} did not replay", seed.0);
    }
}

#[test]
fn proptest_transition_equivalence_regression_seeds() {
    for seed in REGRESSION_SEEDS {
        let first = run_micro(seed).unwrap();
        assert_eq!(first, run_micro(seed).unwrap(), "fixed seed {seed}");
    }
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(40))]

    #[test]
    fn proptest_larger_scopes(case in any::<LargerScopeCase>()) {
        let first = run_larger(case);
        prop_assert!(first.is_ok(), "sample {:?} failed: {:?}", case, first);
        let first = first.unwrap();
        let replay = run_larger(case).unwrap();
        prop_assert_eq!(&first, &replay, "sample {:?} did not replay", case);
        prop_assert_eq!(first.confidence, "sampled");
        prop_assert_eq!(first.players, case.table.players());
    }
}

#[test]
fn proptest_larger_scopes_regression_seeds() {
    for seed in REGRESSION_SEEDS {
        for table in [
            SampledTable::Two,
            SampledTable::Three,
            SampledTable::Seven,
            SampledTable::FiftyOne,
        ] {
            let case = LargerScopeCase { seed, table };
            let first = run_larger(case).unwrap();
            assert_eq!(first, run_larger(case).unwrap(), "fixed sample {case:?}");
            assert_eq!(first.confidence, "sampled");
        }
    }
}

fn run_micro(seed: u64) -> Result<MicroSummary, String> {
    let mut random = SplitMix64(seed);
    let dealer = if random.next() & 1 == 0 {
        Player::Zero
    } else {
        Player::One
    };
    let mut game = Game::new(dealer);
    let mut actions = Vec::with_capacity(20);
    let mut phases = vec![game.phase()];
    let catalog = property_catalog();
    for _ in 0..20 {
        game.validate().map_err(|error| error.to_string())?;
        let action = match game.legal_actions() {
            LegalActions::Chance(choices) => {
                ModelAction::Chance(choices[random.index(choices.len())])
            }
            LegalActions::Player(choices) => {
                ModelAction::Player(choices[random.index(choices.len())])
            }
            LegalActions::Environment => ModelAction::Settle,
            LegalActions::Finished => break,
        };
        direct_action_precondition(game, action)?;
        let transition = game
            .transition(action)
            .map_err(|error| format!("strict transition rejected legal action: {error}"))?;
        let replay = game
            .transition(action)
            .map_err(|error| format!("strict replay rejected legal action: {error}"))?;
        if transition != replay {
            return Err("strict transition replay changed its result".to_owned());
        }
        verify_direct_projection(game, &transition)?;
        for property in &catalog {
            if !evaluate_transition_property(property.id, game, &transition)
                .map_err(|error| error.to_string())?
            {
                return Err(format!("local property {:?} failed", property.id));
            }
        }
        actions.push(action);
        game = transition.next;
        phases.push(game.phase());
    }
    let Game::Finished(finished) = game else {
        return Err(format!("sampled micro trace did not finish: {game:?}"));
    };
    let absorbed = game
        .transition(ModelAction::Absorb)
        .map_err(|error| error.to_string())?;
    if absorbed.next != game {
        return Err("terminal absorb changed state".to_owned());
    }
    Ok(MicroSummary {
        actions,
        phases,
        scores: finished.scores().map(crate::Score::get),
        pot_cents: finished.pot().cents(),
        winners: finished.winners(),
    })
}

fn direct_action_precondition(game: Game, action: ModelAction) -> Result<(), String> {
    match (game, action) {
        (Game::AwaitingDeal(state), ModelAction::Chance(ChanceAction::Deal(deal))) => deal
            .validate_round(state.ledger().round())
            .map_err(|error| error.to_string()),
        (Game::Bidding(state), ModelAction::Player(PlayerAction::Bid { player, bid })) => {
            if player == state.actor() && bid.get() <= state.ledger().round().hand_size() {
                Ok(())
            } else {
                Err("direct bid precondition failed".to_owned())
            }
        }
        (Game::Playing(state), ModelAction::Player(PlayerAction::Play { player, card })) => {
            if player != state.actor() || !state.hand(player).contains(card) {
                return Err("direct play owner/hand precondition failed".to_owned());
            }
            if let Some(lead) = state.trick().lead()
                && state.hand(player).contains_suit(lead.card.suit())
                && card.suit() != lead.card.suit()
            {
                return Err("direct follow-suit precondition failed".to_owned());
            }
            Ok(())
        }
        (Game::Scoring(_), ModelAction::Settle) | (Game::Finished(_), ModelAction::Absorb) => {
            Ok(())
        }
        _ => Err("direct phase/action ownership failed".to_owned()),
    }
}

fn verify_direct_projection(before: Game, transition: &Transition) -> Result<(), String> {
    if let (
        Game::Playing(state),
        ModelAction::Player(PlayerAction::Play { player, card }),
        Some(lead),
    ) = (before, transition.action, playing_lead(before))
    {
        let winner = if direct_second_wins(lead.card, card, state.trump()) {
            player
        } else {
            lead.player
        };
        let before_count = state.tricks_won()[winner.index()].get();
        let after_counts = match transition.next {
            Game::Playing(after) => after.tricks_won(),
            Game::Scoring(after) => after.tricks_won(),
            _ => return Err("completed trick reached wrong phase".to_owned()),
        };
        if after_counts[winner.index()].get() != before_count + 1 {
            return Err("Weavy winner differs from direct winner formula".to_owned());
        }
    }
    if let Game::Scoring(state) = before {
        let events = transition
            .round_scores
            .ok_or_else(|| "settlement omitted raw scores".to_owned())?;
        for player in Player::ALL {
            let expected = direct_score(
                state.bids()[player.index()],
                state.tricks_won()[player.index()].get(),
                state.ledger().round().hand_size(),
            );
            let actual = events[player.index()];
            if (actual.outcome, actual.points, actual.payment_cents) != expected {
                return Err(format!(
                    "Weavy score differs from direct table for {player:?}: {actual:?}"
                ));
            }
        }
    }
    Ok(())
}

fn playing_lead(game: Game) -> Option<crate::PlayedCard> {
    let Game::Playing(state) = game else {
        return None;
    };
    state.trick().lead()
}

fn direct_second_wins(lead: crate::Card, second: crate::Card, trump: crate::Card) -> bool {
    (second.suit() == trump.suit() && lead.suit() != trump.suit())
        || (second.suit() == lead.suit() && second.rank() > lead.rank())
}

fn direct_score(bid: Bid, tricks: u8, hand_size: u8) -> (RoundOutcome, u8, u8) {
    if bid.get() != tricks {
        (RoundOutcome::Miss, 0, 10)
    } else if tricks == hand_size {
        (RoundOutcome::AllTricks, 20 + bid.get(), 0)
    } else {
        (RoundOutcome::Exact, 10 + bid.get(), 0)
    }
}

fn run_larger(case: LargerScopeCase) -> Result<FullDeckSummary, String> {
    match case.table {
        SampledTable::Two => run_full_deck::<2>(case.seed),
        SampledTable::Three => run_full_deck::<3>(case.seed),
        SampledTable::Seven => run_full_deck::<7>(case.seed),
        SampledTable::FiftyOne => run_full_deck::<51>(case.seed),
    }
}

fn run_full_deck<const PLAYERS: usize>(seed: u64) -> Result<FullDeckSummary, String> {
    let mut random = SplitMix64(seed);
    let dealer = oracle::Seat::new(random.index(PLAYERS)).map_err(|error| format!("{error:?}"))?;
    let mut game = oracle::Game::<PLAYERS>::new(dealer).map_err(|error| format!("{error:?}"))?;
    let mut steps = 0;
    loop {
        game.validate().map_err(|error| format!("{error:?}"))?;
        match game.turn() {
            oracle::Turn::Chance => {
                let deck = shuffled_deck(&mut random)?;
                game = game
                    .transition(oracle::Action::Deal(deck))
                    .map_err(|error| format!("{error:?}"))?
                    .next;
            }
            oracle::Turn::Player(_) => {
                let choices = game.legal_player_actions();
                let action = choices[random.index(choices.len())].clone();
                game = game
                    .transition(action)
                    .map_err(|error| format!("{error:?}"))?
                    .next;
            }
            oracle::Turn::Environment => {
                let transition = game
                    .transition(oracle::Action::SettleRound)
                    .map_err(|error| format!("{error:?}"))?;
                if transition.round_scores.is_none() {
                    return Err("full-deck settlement omitted raw scores".to_owned());
                }
                game = transition.next;
            }
            oracle::Turn::Finished => break,
        }
        steps += 1;
        if steps > 10_000 {
            return Err("sampled full-deck trace exceeded step bound".to_owned());
        }
    }
    let oracle::GameState::Finished(finished) = game.state() else {
        return Err("full-deck trace stopped outside Finished".to_owned());
    };
    Ok(FullDeckSummary {
        confidence: "sampled",
        seed,
        players: PLAYERS,
        steps,
        scores: finished.scores.to_vec(),
        pot_cents: finished.pot_cents,
        winners: finished.winners.to_vec(),
    })
}

fn shuffled_deck(random: &mut SplitMix64) -> Result<oracle::DeckOrder, String> {
    let mut cards = oracle::Card::standard_deck();
    for upper in (1..cards.len()).rev() {
        let selected = random.index(upper + 1);
        cards.swap(upper, selected);
    }
    oracle::DeckOrder::new(cards).map_err(|error| format!("{error:?}"))
}

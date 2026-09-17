// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pure authority replay helpers: real `OracleEnvironment` transitions, no DB host.
use poche_environment::{
    EnvironmentAction, GameEnvironment, OracleChanceAction, OracleEnvironment, OraclePlayerAction,
};
use poche_oracle_rust::{Card, Game, PhaseTag, Seat};

pub(super) fn deal(game: &Game<2>, seed: u64) -> Result<Game<2>, String> {
    let round = game.observe(Seat::new(0).unwrap()).round_index;
    let ordinal = u32::try_from(round).map_err(|_| "round ordinal exceeds u32")?;
    OracleEnvironment::<2>::transition(
        game,
        EnvironmentAction::Chance(OracleChanceAction::seeded(seed, ordinal)),
    )
    .map(|result| result.state)
    .map_err(|error| format!("Poche rule rejected the action: {error:?}"))
}

pub(super) fn replay_action(
    game: &Game<2>,
    seed: u64,
    seat: u8,
    kind: &str,
    value: u8,
) -> Result<Game<2>, String> {
    let player =
        Seat::new(usize::from(seat)).map_err(|error| format!("invalid replay seat: {error:?}"))?;
    let action = match kind {
        "bid" => EnvironmentAction::Player(OraclePlayerAction::Bid {
            player,
            tricks: value,
        }),
        "play" => EnvironmentAction::Player(OraclePlayerAction::Play {
            player,
            card: *Card::standard_deck()
                .get(usize::from(value))
                .ok_or("invalid replay card")?,
        }),
        "settle" => EnvironmentAction::Settle,
        "deal" => return deal(game, seed),
        _ => return Err("game action log contains an unknown action".into()),
    };
    OracleEnvironment::<2>::transition(game, action)
        .map(|result| result.state)
        .map_err(|error| format!("Poche rule rejected the action: {error:?}"))
}

pub(super) fn scored_totals(game: &Game<2>) -> [u16; 2] {
    let view = game.observe(Seat::new(0).unwrap());
    if view.phase == PhaseTag::Scoring {
        std::array::from_fn(|seat| {
            view.scores[seat]
                + poche_oracle_rust::score_round(
                    view.bids[seat].unwrap(),
                    view.tricks_won[seat],
                    view.hand_size,
                )
                .points
        })
    } else {
        view.scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_oracle_rust::Turn;

    fn complete_round(mut game: Game<2>) -> Game<2> {
        loop {
            let action = match game.turn() {
                Turn::Player(player) if game.phase() == PhaseTag::Bidding => {
                    OraclePlayerAction::Bid { player, tricks: 0 }
                }
                Turn::Player(_) => OracleEnvironment::<2>::legal_actions(&game)
                    .into_iter()
                    .next()
                    .unwrap(),
                _ => return game,
            };
            game = OracleEnvironment::<2>::transition(&game, EnvironmentAction::Player(action))
                .unwrap()
                .state;
        }
    }

    #[test]
    fn scoring_is_an_explicit_environment_boundary_not_a_players_turn() {
        let initial = OracleEnvironment::<2>::initial(Seat::new(0).unwrap()).unwrap();
        let scored = complete_round(deal(&initial, 7).unwrap());
        assert_eq!(scored.phase(), PhaseTag::Scoring);
        assert_eq!(scored.turn(), Turn::Environment);
        assert_eq!(scored_totals(&scored).iter().sum::<u16>(), 10);
        assert!(deal(&scored, 7).is_err());
    }

    #[test]
    fn settle_then_deal_rotates_dealer_preserves_scores_and_uses_two_card_hands() {
        let initial = OracleEnvironment::<2>::initial(Seat::new(0).unwrap()).unwrap();
        let scored = complete_round(deal(&initial, 7).unwrap());
        let totals = scored_totals(&scored);
        let waiting = replay_action(&scored, 7, 0, "settle", 0).unwrap();
        assert_eq!(waiting.phase(), PhaseTag::AwaitingDeal);
        assert_eq!(
            waiting.observe(Seat::new(0).unwrap()).dealer,
            Some(Seat::new(1).unwrap())
        );
        assert!(replay_action(&waiting, 7, 0, "settle", 0).is_err());
        let second = replay_action(&waiting, 7, 1, "deal", 0).unwrap();
        let view = second.observe(Seat::new(0).unwrap());
        assert_eq!(view.round_index, 1);
        assert_eq!(view.hand_counts, [2, 2]);
        assert_eq!(view.scores, totals);
        let second_scored = complete_round(second);
        assert_eq!(second_scored.phase(), PhaseTag::Scoring);
        assert_eq!(
            second_scored
                .observe(Seat::new(0).unwrap())
                .tricks_won
                .iter()
                .sum::<u8>(),
            2
        );
    }

    #[test]
    fn every_scheduled_round_settles_once_and_final_state_cannot_redeal() {
        let mut game = OracleEnvironment::<2>::initial(Seat::new(0).unwrap()).unwrap();
        let rounds = poche_oracle_rust::round_count(2);
        for index in 0..rounds {
            assert_eq!(game.phase(), PhaseTag::AwaitingDeal);
            let view = game.observe(Seat::new(0).unwrap());
            assert_eq!(view.round_index, index);
            assert_eq!(view.dealer.unwrap().index(), index % 2);
            game = complete_round(deal(&game, 8128).unwrap());
            let totals = scored_totals(&game);
            game = replay_action(&game, 8128, 0, "settle", 0).unwrap();
            assert_eq!(game.observe(Seat::new(0).unwrap()).scores, totals);
            assert!(replay_action(&game, 8128, 0, "settle", 0).is_err());
        }
        assert_eq!(game.phase(), PhaseTag::Finished);
        assert!(deal(&game, 8128).is_err());
    }

    #[test]
    fn replayed_bid_play_settle_deal_log_recreates_private_hands_and_public_totals() {
        let seed = 4242;
        let initial = OracleEnvironment::<2>::initial(Seat::new(0).unwrap()).unwrap();
        let mut original = deal(&initial, seed).unwrap();
        let replay_root = original.clone();
        let mut log = Vec::new();
        while original.observe(Seat::new(0).unwrap()).round_index < 2 {
            let (seat, kind, value, action) = match original.turn() {
                Turn::Player(_) => {
                    let action = OracleEnvironment::<2>::legal_actions(&original)
                        .into_iter()
                        .next()
                        .unwrap();
                    let (seat, kind, value) = match action {
                        OraclePlayerAction::Bid { player, tricks } => {
                            (u8::try_from(player.index()).unwrap(), "bid", tricks)
                        }
                        OraclePlayerAction::Play { player, card } => (
                            u8::try_from(player.index()).unwrap(),
                            "play",
                            u8::try_from(
                                Card::standard_deck()
                                    .iter()
                                    .position(|candidate| *candidate == card)
                                    .unwrap(),
                            )
                            .unwrap(),
                        ),
                    };
                    (seat, kind, value, EnvironmentAction::Player(action))
                }
                Turn::Environment => (0, "settle", 0, EnvironmentAction::Settle),
                Turn::Chance => {
                    log.push((0, "deal", 0));
                    original = deal(&original, seed).unwrap();
                    continue;
                }
                Turn::Finished => panic!("two rounds do not finish the game"),
            };
            log.push((seat, kind, value));
            original = OracleEnvironment::<2>::transition(&original, action)
                .unwrap()
                .state;
        }
        let replayed = log
            .into_iter()
            .fold(replay_root, |game, (seat, kind, value)| {
                replay_action(&game, seed, seat, kind, value).unwrap()
            });
        assert_eq!(original, replayed);
        assert!(replay_action(&replayed, seed, 0, "unknown", 0).is_err());
    }
}

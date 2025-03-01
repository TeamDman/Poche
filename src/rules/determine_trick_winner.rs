use crate::rules::assertions::{assert_active_player_is_some, assert_follow_suits_is_none, assert_follow_suits_is_some, assert_trump_is_some};
use crate::rules::assertions::assert_all_players_bet_is_some;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::OptionExt;
use eyre::bail;

pub struct DetermineTrickWinnerBehaviour;

impl RuleBehaviour for DetermineTrickWinnerBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_some(state);
        assert_trump_is_some(state);
        // Identify the winner
        let Some(trump) = state.get_trump() else {
            bail!("Trump suit missing, how did we get here?");
        };
        let Some(follow_suit) = state.get_suit_to_follow() else {
            bail!("Follow suit missing, how did we get here?");
        };

        println!(
            "Determining winner of the trick. {} was lead, {} was trump.",
            follow_suit, trump
        );

        println!("========");
        let mut played = state.get_played_cards();
        for (card, _, player) in &played {
            println!("{} played {} (value={})", player.id, card, card.value(follow_suit, trump));
        }
        println!("========");

        played.sort_by(|a, b| {
            a.0.value(follow_suit, trump)
                .cmp(&b.0.value(follow_suit, trump)).reverse()
        });
        let (winner_card, winner_index, winner) = played
            .into_iter()
            .next()
            .ok_or_eyre("Could not find winner")?;

        println!("{} won the trick with the {}", winner.id, winner_card);

        // Set the winner as the active player
        state.players.active_player_index = Some(winner_index);

        // Move the pile to the winner
        state.stack.push_front(Rule::MovePileToWinner);
        Ok(())
    }
}

use crate::rules::assertions::assert_active_player_is_some;
use crate::rules::assertions::assert_all_players_bet_is_some;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_follow_suits_is_some;
use crate::rules::assertions::assert_trump_is_some;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::bail;
use itertools::Itertools;

pub struct MovePileToWinnerBehaviour;

impl RuleBehaviour for MovePileToWinnerBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_some(state);
        assert_trump_is_some(state);

        let Some(winner_id) = state.players.active_player_id.clone() else {
            bail!("Could not determine the winner because there is no active player");
        };

        // Move the trick in front of the player
        let trick = state.pile.drain(..).map(|(_, card)| card).collect_vec();
        state.players[&winner_id].tricks.push(trick);
        assert_follow_suits_is_none(state);
        Ok(())
    }
}

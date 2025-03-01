use crate::action::Action;
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

pub struct PlayCardBehaviour;

impl RuleBehaviour for PlayCardBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_trump_is_some(state);
        let (active_player_index, active_player) = state.players.get_active_player()?;
        let actions = Action::get_play_card_actions(&state)?;
        if actions.is_empty() {
            bail!("No actions available for player {}", active_player.id);
        }
        let chosen_action = active_player.policy.pick_action(&mut state.rand, actions);
        chosen_action.apply(state);
        
        // Advance the active player
        state.players.active_player_index = Some(
            state
                .players
                .get_wrapped(active_player_index as isize + 1)
                .0,
        );

        assert_follow_suits_is_some(state);
        Ok(())
    }
}

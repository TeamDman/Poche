use crate::action::PlayCardAction;
use crate::rules::assertions::assert_active_player_is_some;
use crate::rules::assertions::assert_all_players_bet_is_some;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_some;
use crate::rules::assertions::assert_trump_is_some;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct PlayCardBehaviour;

impl RuleBehaviour for PlayCardBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_trump_is_some(state);

        let (_, active_player) = state.players.get_active_player()?;
        let choices = PlayCardAction::get_valid_choices(&state)?;

        let mut rand = state.rand.clone();
        let chosen_action = active_player.policy.play_card(&mut rand, choices, &state)?;
        state.rand = rand;

        chosen_action.apply(state)?;

        // Advance the active player
        state.players.advance_active_player()?;

        assert_follow_suits_is_some(state);
        Ok(())
    }
}

use crate::actions::place_bet_action::BetAction;
use crate::rules::assertions::assert_active_player_is_some;
use crate::rules::assertions::assert_cards_only_in_deck_or_hands;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_trump_is_some;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct BetBehaviour;

impl RuleBehaviour for BetBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_some(state);

        // Get valid choices
        let choices = BetAction::get_valid_choices(state)?;

        // Pick based on player policy
        let mut rand = state.rand;
        let (_, active_player) = state.players.get_active_player()?;
        let chosen_action = active_player.place_bet(&mut rand, choices, state)?;
        state.rand = rand;

        // Apply the bet
        chosen_action.apply(state)?;

        // Advance the active player
        state.players.advance_active_player()?;

        Ok(())
    }
}

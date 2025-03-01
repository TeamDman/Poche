use crate::rules::assertions::{assert_active_player_is_none, assert_active_player_is_some, assert_follow_suits_is_none, assert_trump_is_none};
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::rule::Rule::DetermineWinner;
use crate::rules::rule::Rule::PassDealer;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct NextRoundBehaviour;

impl RuleBehaviour for NextRoundBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_none(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_cards_only_in_deck(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);

        if state.round.is_last_round() {
            state.stack.push_back(DetermineWinner);
        } else {
            state.round.try_advance(state.players.len() as u32)?;
            state.stack.push_back(PassDealer);
        }

        Ok(())
    }
}

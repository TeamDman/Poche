use crate::rules::assertions::{assert_active_player_is_none, assert_active_player_is_some, assert_follow_suits_is_none, assert_follow_suits_is_some, assert_trump_is_none, assert_trump_is_some};
use crate::rules::assertions::assert_all_players_bet_is_some;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_player_hands_empty;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct RoundOverBehaviour;

impl RuleBehaviour for RoundOverBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_player_hands_empty(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_some(state);
        
        // Clear the round state
        state.players.active_player_index = None;
        state.deck.push(state.trump.take().unwrap());
        
        state.stack.push_back(Rule::DetermineBetOutcomes);
        
        assert_trump_is_none(state);
        assert_active_player_is_none(state);
        Ok(())
    }
}

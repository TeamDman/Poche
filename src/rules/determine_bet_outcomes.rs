use crate::action::Action;
use crate::rules::rule::{Rule, RuleBehaviour};
use crate::state::State;
use eyre::OptionExt;
use eyre::bail;
use itertools::Itertools;
use crate::rules::assertions::{assert_active_player_is_none, assert_active_player_is_some, assert_all_players_bet_is_some, assert_dealer_is_some, assert_follow_suits_is_none, assert_player_hands_empty, assert_trump_is_none, assert_trump_is_some};

pub struct DetermineBetOutcomesBehaviour;

impl RuleBehaviour for DetermineBetOutcomesBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_player_hands_empty(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);
        
        for _ in 0..state.players.len() {
            state.stack.push_back(Rule::UpdateBetOutcome);
        }
        state.stack.push_back(Rule::NextRound);
        Ok(())
    }
}

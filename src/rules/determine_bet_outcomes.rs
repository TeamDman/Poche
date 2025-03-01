use crate::rules::assertions::assert_active_player_is_none;
use crate::rules::assertions::assert_all_players_bet_is_some;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_player_hands_empty;
use crate::rules::assertions::assert_trump_is_none;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

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

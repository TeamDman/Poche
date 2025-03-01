use crate::rules::assertions::{assert_active_player_is_none, assert_all_players_bet_is_none, assert_all_players_bet_is_some, assert_cards_only_in_deck_or_hands, assert_dealer_is_some, assert_follow_suits_is_none, assert_trump_is_some};
use crate::rules::rule::{Rule, RuleBehaviour};
use crate::state::State;

pub struct PlayRoundBehaviour;
impl RuleBehaviour for PlayRoundBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_all_players_bet_is_some(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_some(state);
        for _ in 0..state.round.hand_size {
            state.stack.push_back(Rule::PlayTrick);
        }
        state.players.active_player_index = Some(state.players.get_left_of_dealer()?.0);
        state.stack.push_back(Rule::RoundOver);
        Ok(())
    }
}

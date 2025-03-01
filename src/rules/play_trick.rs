use crate::rules::assertions::{assert_active_player_is_none, assert_active_player_is_some, assert_all_players_bet_is_none, assert_all_players_bet_is_some, assert_cards_only_in_deck_or_hands, assert_dealer_is_some, assert_follow_suits_is_none, assert_pile_empty, assert_trump_is_some};
use crate::rules::rule::{Rule, RuleBehaviour};
use crate::state::State;

pub struct PlayTrickBehaviour;
impl RuleBehaviour for PlayTrickBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_some(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_pile_empty(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_some(state);
        state.stack.push_front(Rule::DetermineTrickWinner);
        for _ in 0..state.players.len() {
            state.stack.push_front(Rule::PlayCard);
        }
        Ok(())
    }
}

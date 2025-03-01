use crate::rules::assertions::{assert_active_player_is_none, assert_follow_suits_is_none, assert_trump_is_none};
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct ShuffleBehaviour;
impl RuleBehaviour for ShuffleBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck(state);
        assert_all_players_bet_is_none(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);
        state.rand.shuffle(&mut state.deck);
        state.stack.push_front(Rule::DealHands);
        Ok(())
    }
}

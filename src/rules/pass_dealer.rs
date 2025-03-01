use crate::rules::assertions::assert_active_player_is_none;
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_trump_is_none;
use crate::rules::rule::Rule::Shuffle;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct PassDealerBehaviour;

impl RuleBehaviour for PassDealerBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_all_players_bet_is_none(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_cards_only_in_deck(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);

        let dealer_index = state.players.dealer_index.unwrap();
        let next_dealer_index = (dealer_index + 1) % state.players.len();
        state.players.dealer_index = Some(next_dealer_index);

        state.stack.push_back(Shuffle);

        Ok(())
    }
}

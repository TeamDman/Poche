use crate::rules::assertions::assert_active_player_is_none;
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck;
use crate::rules::assertions::assert_dealer_is_none;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_trump_is_none;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;

pub struct DetermineDealerBehaviour;
impl RuleBehaviour for DetermineDealerBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck(state);
        assert_all_players_bet_is_none(state);
        assert_dealer_is_none(state);
        assert_active_player_is_none(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);

        let dealer_index = state.rand.gen_range(0..state.players.len());
        state.players.dealer_index = Some(dealer_index);
        state.stack.push_front(Rule::Shuffle);
        Ok(())
    }
}

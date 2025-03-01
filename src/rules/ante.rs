use crate::money::Coin;
use crate::money::MoneyJar;
use crate::rules::assertions::{assert_all_players_bet_is_none, assert_cards_only_in_deck, assert_active_player_is_none, assert_dealer_is_none, assert_follow_suits_is_none, assert_trump_is_none};
use crate::rules::rule::{Rule, RuleBehaviour};
use crate::state::State;

pub struct AnteBehaviour;
impl RuleBehaviour for AnteBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck(state);
        assert_all_players_bet_is_none(state);
        assert_dealer_is_none(state);
        assert_active_player_is_none(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);
        for player in state.players.iter_mut() {
            player.money_jar -= MoneyJar::from(vec![Coin::Quarter]);
            state.pot += MoneyJar::from(vec![Coin::Quarter]);
        }
        state.stack.push_front(Rule::DetermineDealer);
        Ok(())
    }
}

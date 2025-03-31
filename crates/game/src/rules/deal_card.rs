use crate::rules::assertions::assert_active_player_is_some;
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck_or_hands;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_trump_is_none;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::OptionExt;

pub struct DealCardBehaviour;
impl RuleBehaviour for DealCardBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_all_players_bet_is_none(state);
        assert_active_player_is_some(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);

        let (_, active_player) = state.players.get_active_player_mut()?;
        let card = state.deck.pop().ok_or_eyre("No cards left to deal")?;
        active_player.hand.push(card);
        state.players.advance_active_player()?;

        Ok(())
    }
}

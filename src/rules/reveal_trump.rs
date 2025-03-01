use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::OptionExt;
use crate::rules::assertions::{assert_active_player_is_none, assert_all_players_bet_is_none, assert_cards_only_in_deck_or_hands, assert_dealer_is_some, assert_follow_suits_is_none, assert_trump_is_none};

pub struct RevealTrumpBehaviour;
impl RuleBehaviour for RevealTrumpBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_all_players_bet_is_none(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);
        state.trump = Some(
            state
                .deck
                .pop()
                .ok_or_eyre("Failed to reveal trump by taking the top card of the deck")?,
        );
        state.stack.push_back(Rule::CollectBets);
        Ok(())
    }
}

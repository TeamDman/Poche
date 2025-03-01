use crate::rules::assertions::{assert_active_player_is_none, assert_follow_suits_is_none, assert_trump_is_none};
use crate::rules::assertions::assert_all_players_bet_is_none;
use crate::rules::assertions::assert_cards_only_in_deck_or_hands;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::OptionExt;

pub struct DealCardBehaviour;
impl RuleBehaviour for DealCardBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_all_players_bet_is_none(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);
        let target_hand_size = state.round.hand_size; // Ensures consistency with round rules

        loop {
            // Find the player clockwise from the dealer with the least cards.
            let mut next_player_to_receive_card = None;
            let mut min_hand_size = usize::MAX;

            for (i, player) in state.players.iter_dealer_last()? {
                let hand_size = player.hand.len();
                if hand_size < target_hand_size as usize && hand_size < min_hand_size {
                    next_player_to_receive_card = Some(i);
                    min_hand_size = hand_size;
                }
            }

            match next_player_to_receive_card {
                Some(i) => {
                    let card = state.deck.pop().ok_or_eyre("No cards left to deal")?;
                    state.players[i].hand.push(card);
                }
                None => break, // No more players need cards
            }
        }
        Ok(())
    }
}

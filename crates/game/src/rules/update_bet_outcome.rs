use crate::rules::assertions::assert_active_player_is_none;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_player_hands_empty;
use crate::rules::assertions::assert_trump_is_none;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::bail;

pub struct UpdateBetOutcomeBehaviour;

impl RuleBehaviour for UpdateBetOutcomeBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_player_hands_empty(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_none(state);

        // Find the next player who has a bet to be evaluated
        let Some((player_index, _)) = state
            .players
            .iter_dealer_last()?
            .find(|(_player_index, player)| player.bet.is_some())
        else {
            bail!("All bets have already been resolved");
        };

        // Grab the player
        let player = state.players.get_mut(player_index).unwrap();
        let bet = player.bet.unwrap();
        let tricks = player.tricks.len() as u32;
        let hand_size = state.round.hand_size;

        // Calculate the score change
        match (bet, tricks, hand_size) {
            (bet, taken, all) if bet == taken && taken == all => {
                player.score += 20 + bet;
            }
            (bet, taken, _) if bet == taken => {
                player.score += 10 + bet;
            }
            (bet, taken, _) if bet != taken => {
                player.score += 0;
            }
            x => unreachable!("invalid state for (bet,taken,all) calculation: {:?}", x),
        }

        // Clear the bet
        player.bet = None;

        // Return the cards to the deck
        for trick in player.tricks.drain(..) {
            state.deck.extend(trick);
        }

        Ok(())
    }
}

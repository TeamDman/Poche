use crate::action::Action;
use crate::rules::assertions::assert_active_player_is_none;
use crate::rules::assertions::assert_cards_only_in_deck_or_hands;
use crate::rules::assertions::assert_dealer_is_some;
use crate::rules::assertions::assert_follow_suits_is_none;
use crate::rules::assertions::assert_trump_is_some;
use crate::rules::rule::RuleBehaviour;
use crate::state::State;
use eyre::bail;
use itertools::Itertools;

pub struct BetBehaviour;

impl RuleBehaviour for BetBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        assert_cards_only_in_deck_or_hands(state);
        assert_active_player_is_none(state);
        assert_dealer_is_some(state);
        assert_follow_suits_is_none(state);
        assert_trump_is_some(state);
        let mut next_bettor_index = None;
        for (i, player) in state.players.iter_dealer_last()? {
            if player.bet.is_none() {
                next_bettor_index = Some(i);
                break;
            }
        }
        let Some(next_bettor_index) = next_bettor_index else {
            bail!("All players already placed their bet");
        };
        let player = &state.players[next_bettor_index];
        let actions = (0..=player.hand.len())
            .map(|i| Action::Bet {
                player_index: next_bettor_index,
                tricks: i as u32,
            })
            .collect_vec();
        let bet_amount = player.policy.pick_action(&mut state.rand, actions);
        bet_amount.apply(state);
        Ok(())
    }
}

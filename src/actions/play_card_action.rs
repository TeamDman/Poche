use std::fmt::Formatter;
use crate::cards::Card;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlayCardAction {
    pub card_index: usize,
    pub card: Card,
}
impl PlayCardAction {
    pub fn get_valid_choices(state: &State) -> eyre::Result<Vec<PlayCardAction>> {
        let (_, active_player) = state.players.get_active_player()?;
        let mut choices: Vec<PlayCardAction> = active_player
            .hand
            .iter()
            .cloned()
            .enumerate()
            .map(|(card_index, card)| PlayCardAction { card, card_index })
            .collect();
        if let Some(suit) = state.get_suit_to_follow() {
            let can_follow_suit = active_player.hand.iter().any(|c| c.suit == suit);
            if can_follow_suit {
                choices.retain(|action| action.card.suit == suit);
            }
        }
        Ok(choices)
    }
    pub fn apply(&self, state: &mut State) -> eyre::Result<()> {
        let (_, player) = state.players.get_active_player_mut()?;
        let card = player.hand.remove(self.card_index);
        assert_eq!(card, self.card);
        state.pile.push(card);
        Ok(())
    }
}
impl std::fmt::Display for PlayCardAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Play {}", self.card))
    }
}

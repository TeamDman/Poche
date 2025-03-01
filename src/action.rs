use eyre::bail;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Action {
    PlayCard { player_index: usize, card_index: usize },
    Bet { player_index: usize, tricks: u32 },
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Action {
    pub fn get_play_card_actions(state: &State) -> eyre::Result<Vec<Action>> {
        let active_player = &state.players[state.players.active_player_index.unwrap()];
        assert!(!active_player.hand.is_empty());
        let mut actions: Vec<Action> = active_player
            .hand
            .iter()
            .enumerate()
            .map(|(card_index, _)| Action::PlayCard {
                player_index: state.players.active_player_index.unwrap(),
                card_index,
            })
            .collect();
        match state.get_suit_to_follow() {
            Some(suit) => {
                let can_follow_suit = active_player.hand.iter().any(|c| c.suit == suit);
                if can_follow_suit {
                    actions.retain(|action| {
                        let Action::PlayCard { card_index, .. } = action else {
                            eprintln!("Invalid action: {}", action);
                            return false;
                        };
                        {
                            active_player.hand[*card_index].suit == suit
                        }
                    });
                }
            },
            None => {}
        };
        Ok(actions)
    }
    pub fn apply(&self, state: &mut State) {
        match self {
            Action::PlayCard { player_index: player, card_index } => {
                let card = state.players[*player].hand.remove(*card_index);
                state.pile.push(card);
            }
            Action::Bet { player_index: player, tricks } => {
                state.players[*player].bet = Some(*tricks);
            }
        }
    }
}

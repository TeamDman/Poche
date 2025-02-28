use crate::state::GamePhase;
use crate::state::State;
use eyre::bail;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Action {
    PlayCard { player: usize, card_index: usize },
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Action {
    pub fn get_valid_actions(state: &State) -> eyre::Result<Vec<Action>> {
        let active_player = match state.phase {
            GamePhase::Playing {
                active_player_index: active_player,
                ..
            } => active_player,
            _ => bail!("No active player found"),
        };

        let player = &state.players[active_player];
        Ok(player
            .hand
            .iter()
            .enumerate()
            .map(|(card_index, _)| Action::PlayCard {
                player: active_player,
                card_index,
            })
            .collect())
    }
    pub fn apply(&self, state: &mut State) {
        match self {
            Action::PlayCard { player, card_index } => {
                let card = state.players[*player].hand.remove(*card_index);
                state.pile.push(card);
            }
        }
    }
}

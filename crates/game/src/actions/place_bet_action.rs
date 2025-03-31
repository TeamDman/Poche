use crate::state::State;
use std::fmt::Formatter;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BetAction {
    pub tricks: u32,
}
impl BetAction {
    pub fn get_valid_choices(state: &State) -> eyre::Result<Vec<BetAction>> {
        let active_player = state.players.get_active_player()?;
        let mut choices = Vec::new();
        for i in 0..=active_player.hand.len() {
            choices.push(BetAction { tricks: i as u32 });
        }
        Ok(choices)
    }
    pub fn apply(&self, state: &mut State) -> eyre::Result<()> {
        let player = state.players.get_active_player_mut()?;
        player.bet = Some(self.tricks);
        Ok(())
    }
}
impl std::fmt::Display for BetAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Bet {}", self.tricks))
    }
}

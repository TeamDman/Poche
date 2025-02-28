use eyre::bail;
use poche::state::State;

#[test]
fn bruh() -> eyre::Result<()> {
    let mut state = State::default();
    let num_players = state.players.len();
    for _ in 0..10000 {
        assert!(state.pile.is_empty());
        for _ in 0..num_players {
            state.step()?;
        }
        assert_eq!(state.pile.len(), 0);
        if state.is_done() {
            return Ok(());
        }
    }
    bail!("Timeout limit reached");
}
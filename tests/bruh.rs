use eyre::bail;
use poche::state::State;

#[test]
fn it_works() -> eyre::Result<()> {
    let num_players = 4;
    let mut state = State::new_with_random_players(num_players);
    for _ in 0..10000 {
        match state.tick()? {
            Some(_action) => {
                // println!("{:?}", action);
            }
            None => return Ok(()),
        }
    }
    bail!("Timeout limit reached");
}

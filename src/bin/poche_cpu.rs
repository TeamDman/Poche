use poche::state::State;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    
    let num_players = 4;
    let mut state = State::new_with_random_players(num_players);
    loop {
        match state.tick()? {
            Some(_action) => {
                // println!("{:?}", action);
            }
            None => return Ok(()),
        }
    }
}
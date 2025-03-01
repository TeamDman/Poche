use poche::state::State;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let mut state = State::new_with_random_players(4);
    println!("{state}");
    println!("Begin game loop");
    loop {
        let action_taken = state.tick()?;
        if action_taken.is_none() {
            break;
        }
    }
    println!("Game loop ended");
    println!("{state}");
    Ok(())
}

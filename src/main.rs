use state::State;

mod action;
pub mod cards;
mod money;
pub mod players;
mod policy;
pub mod random;
mod round;
mod state;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    println!("Hi");
    let mut state = State::default();
    loop {
        println!("State: {state}");
        let action_taken = state.step()?;
        println!("Action: {action_taken:?}\n");
        if state.is_done() {
            println!("Game has ended!");
            break;
        }
    }
    println!("{state}");
    Ok(())
}

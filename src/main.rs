use poche::rules::rule::Rule;
use poche::state::State;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let mut state = State::new_with_random_players(4);
    println!("{state}");
    println!("Begin game loop");
    let mut player_score_timeline: Vec<Vec<u32>> = Vec::new();
    let mut hand_size_timeline: Vec<u32> = Vec::new();
    loop {
        match state.tick()? {
            None => break,
            Some(Rule::RoundOver) => {
                hand_size_timeline.push(state.round.hand_size);
            }
            Some(Rule::NextRound) => {
                player_score_timeline
                    .push(state.players.iter().map(|player| player.score).collect());
            }
            _ => {}
        }
    }
    println!("Game loop ended");
    println!("{state}");
    println!();
    println!("Scorecard:");
    // print player names as table
    print!("{:<15}", "Hand Size");
    for player in state.players.iter() {
        print!("{:<15}", player.id.to_string());
    }
    println!();
    // print player scores as table
    for (i, (scores, hand_size)) in player_score_timeline.iter().zip(hand_size_timeline.iter()).enumerate() {
        print!("{:<15}", hand_size);
        for (j, score) in scores.iter().enumerate() {
            let previous_score = if i == 0 { 0 } else { player_score_timeline[i - 1][j] };
            let score_change = score - previous_score;
            print!("{:<15}", score_change);
        }
        println!();
    }
    // print final scores
    println!("{}", "=".repeat(15*(state.players.len()+1)));
    print!("{:<15}", "Final Score");
    for player in state.players.iter() {
        print!("{:<15}", player.score);
    }
    Ok(())
}

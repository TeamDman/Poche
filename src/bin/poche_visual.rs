#![feature(try_blocks)]
use macroquad::experimental::coroutines::TimerDelayFuture;
use macroquad::file::set_pc_assets_folder;
use macroquad::prelude::coroutines::start_coroutine;
use macroquad::prelude::coroutines::wait_seconds;
use macroquad::window::next_frame;
use poche::state::State;
use poche::visualizer::state_visualizer::StateVisualizer;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;

#[macroquad::main("Poche Visualizer")]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    set_pc_assets_folder("assets");
    let state = Arc::new(Mutex::new(State::new_with_random_players(4)));
    let spectating = None;
    let vis = StateVisualizer::new(spectating).await?;
    let quit = Arc::new(Mutex::new(false));
    {
        let state = state.clone();
        let quit = quit.clone();
        start_coroutine(async move {
            let result: eyre::Result<()> = try {
                loop {
                    println!("Ticking game loop");
                    if *quit.lock().map_err(|_| eyre::eyre!("Quit lock poisoned"))? {
                        println!("Quitting game loop");
                        break;
                    }
                    {
                        let mut state = state
                            .lock()
                            .map_err(|_| eyre::eyre!("State lock poisoned"))?;
                        _ = state.tick()?;
                        drop(state);
                    }
                    wait_seconds(1.0).await;
                }
            };
            if let Err(e) = result {
                println!("Render loop error: {:?}", e);
                //noinspection RsUnwrap
                *quit.lock().unwrap() = true;
            }
        });
    }
    let result: eyre::Result<()> = try {
        loop {
            if *quit.lock().map_err(|_| eyre::eyre!("Quit lock poisoned"))? {
                println!("Quitting render loop");
                break;
            }
            let state = state
                .lock()
                .map_err(|_| eyre::eyre!("State lock poisoned"))?;
            println!("Drawing state");
            vis.draw(&state)?;
            drop(state);
            next_frame().await;
        }
    };
    if let Err(e) = result {
        println!("Render loop error: {:?}", e);
        *quit.lock().map_err(|_| eyre::eyre!("Quit lock poisoned"))? = true;
    }
    Ok(())
}

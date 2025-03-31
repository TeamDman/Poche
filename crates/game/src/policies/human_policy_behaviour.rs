use crate::actions::place_bet_action::BetAction;
use crate::actions::play_card_action::PlayCardAction;
use crate::players::{Player, PlayerId};
use crate::policies::policy::PolicyBehaviour;
use crate::random::RandomState;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HumanPolicyBehaviour;

impl HumanPolicyBehaviour {}

impl PolicyBehaviour for HumanPolicyBehaviour {
    fn place_bet(
        &self,
        player: &Player,
        _rand: &mut RandomState,
        mut choices: Vec<BetAction>,
        state: &State,
    ) -> eyre::Result<BetAction> {
        println!("Your turn to bet!");
        println!("Trump: {}", state.trump.unwrap());
        let tricks_bid = state.players.iter().filter_map(|player| player.bet).sum::<u32>();
        println!("Tricks Bid: {tricks_bid}");
        println!("Your hand:");
        for card in player.hand.iter() {
            println!("{card}");
        }

        for (i, action) in choices.iter().enumerate() {
            println!("{}: {}", i, action);
        }
        loop {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();
            if let Ok(action_index) = input.parse::<usize>() {
                let action = choices.swap_remove(action_index);
                break Ok(action);
            }
        }
    }

    fn play_card(
        &self,
        _player: &Player,
        _rand: &mut RandomState,
        mut choices: Vec<PlayCardAction>,
        state: &State,
    ) -> eyre::Result<PlayCardAction> {
        println!("Your turn to play!");
        println!("Trump: {}", state.trump.unwrap());
        println!("Cards played:");
        for card in state.pile.iter() {
            println!("{card}");
        }
        
        println!("Your hand:");
        for (i, action) in choices.iter().enumerate() {
            println!("{}: {}", i, action);
        }
        loop {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();
            if let Ok(action_index) = input.parse::<usize>() {
                let action = choices.swap_remove(action_index);
                break Ok(action);
            }
        }
    }
}
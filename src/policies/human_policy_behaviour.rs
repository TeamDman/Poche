use crate::actions::place_bet_action::BetAction;
use crate::actions::play_card_action::PlayCardAction;
use crate::policies::policy::PolicyBehaviour;
use crate::random::RandomState;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HumanPolicyBehaviour;

impl PolicyBehaviour for HumanPolicyBehaviour {
    fn place_bet(
        &self,
        _rand: &mut RandomState,
        mut choices: Vec<BetAction>,
        _state: &State,
    ) -> eyre::Result<BetAction> {
        println!("Your turn to bet!");
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

    fn play_card(
        &self,
        _rand: &mut RandomState,
        mut choices: Vec<PlayCardAction>,
        _state: &State,
    ) -> eyre::Result<PlayCardAction> {
        println!("Your turn! Choose an action to perform:");
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
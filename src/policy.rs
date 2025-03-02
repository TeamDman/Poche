use crate::action::BetAction;
use crate::action::PlayCardAction;
use crate::random::RandomState;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Policy {
    Random,
    Human,
}
impl Policy {
    pub fn get_behaviour(&self) -> Box<dyn PolicyBehaviour> {
        match self {
            Policy::Random => Box::from(RandomPolicyBehaviour),
            Policy::Human => Box::from(HumanPolicyBehaviour),
        }
    }
    pub fn place_bet(
        &self,
        rand: &mut RandomState,
        mut choices: Vec<BetAction>,
        state: &State,
    ) -> eyre::Result<BetAction> {
        assert_ne!(choices.len(), 0);
        if choices.len() == 1 {
            return Ok(choices.remove(0));
        }
        self.get_behaviour().place_bet(rand, choices, state)
    }
    pub fn play_card(
        &self,
        rand: &mut RandomState,
        mut choices: Vec<PlayCardAction>,
        state: &State,
    ) -> eyre::Result<PlayCardAction> {
        assert_ne!(choices.len(), 0);
        if choices.len() == 1 {
            return Ok(choices.remove(0));
        }
        self.get_behaviour().play_card(rand, choices, state)
    }
}

pub trait PolicyBehaviour {
    fn place_bet(
        &self,
        rand: &mut RandomState,
        choices: Vec<BetAction>,
        state: &State,
    ) -> eyre::Result<BetAction>;
    fn play_card(
        &self,
        rand: &mut RandomState,
        choices: Vec<PlayCardAction>,
        state: &State,
    ) -> eyre::Result<PlayCardAction>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RandomPolicyBehaviour;
impl PolicyBehaviour for RandomPolicyBehaviour {
    fn place_bet(
        &self,
        rand: &mut RandomState,
        mut choices: Vec<BetAction>,
        _state: &State,
    ) -> eyre::Result<BetAction> {
        let (action_index, _action) = rand.pick(&choices);
        let action = choices.remove(action_index);
        Ok(action)
    }

    fn play_card(
        &self,
        rand: &mut RandomState,
        mut choices: Vec<PlayCardAction>,
        _state: &State,
    ) -> eyre::Result<PlayCardAction> {
        let (action_index, _action) = rand.pick(&choices);
        let action = choices.remove(action_index);
        Ok(action)
    }
}

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

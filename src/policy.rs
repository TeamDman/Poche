use crate::action::Action;
use crate::random::RandomState;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Policy {
    Random,
    Human,
}
impl Policy {
    pub fn pick_action(&self, rand: &mut RandomState, mut valid_actions: Vec<Action>) -> Action {
        assert_ne!(valid_actions.len(), 0);
        if valid_actions.len() == 1 {
            return valid_actions.remove(0);
        }
        match self {
            Policy::Random => {
                let (action_index, _action) = rand.pick(&valid_actions);
                let action = valid_actions.remove(action_index);
                action
            }
            Policy::Human => {
                println!("Your turn! Choose an action to perform:");
                for (i,action) in valid_actions.iter().enumerate() {
                    println!("{}: {}", i, action);
                }
                loop {
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    let input = input.trim();
                    if let Ok(action_index) = input.parse::<usize>() {
                        let action = valid_actions.swap_remove(action_index);
                        break action;
                    }
                }
            }
        }
    }
}

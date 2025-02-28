use crate::action::Action;
use crate::random::RandomState;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Policy {
    Random,
}
impl Policy {
    pub fn pick_action(&self, rand: &mut RandomState, mut valid_actions: Vec<Action>) -> Action {
        match self {
            Policy::Random => {
                let (action_index, _action) = rand.pick(&valid_actions);
                let action = valid_actions.remove(action_index);
                action
            }
        }
    }
}

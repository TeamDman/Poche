use crate::actions::place_bet_action::BetAction;
use crate::actions::play_card_action::PlayCardAction;
use crate::players::Player;
use crate::policies::policy::PolicyBehaviour;
use crate::random::RandomState;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RandomPolicyBehaviour;

impl PolicyBehaviour for RandomPolicyBehaviour {
    fn place_bet(
        &self,
        _player: &Player,
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
        _player: &Player,
        rand: &mut RandomState,
        mut choices: Vec<PlayCardAction>,
        _state: &State,
    ) -> eyre::Result<PlayCardAction> {
        let (action_index, _action) = rand.pick(&choices);
        let action = choices.remove(action_index);
        Ok(action)
    }
}

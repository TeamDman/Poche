use crate::actions::place_bet_action::BetAction;
use crate::actions::play_card_action::PlayCardAction;
use crate::players::Player;
use crate::policies::human_policy_behaviour::HumanPolicyBehaviour;
use crate::policies::random_policy_behaviour::RandomPolicyBehaviour;
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
}

pub trait PolicyBehaviour {
    fn place_bet(
        &self,
        player: &Player,
        rand: &mut RandomState,
        choices: Vec<BetAction>,
        state: &State,
    ) -> eyre::Result<BetAction>;
    fn play_card(
        &self,
        player: &Player,
        rand: &mut RandomState,
        choices: Vec<PlayCardAction>,
        state: &State,
    ) -> eyre::Result<PlayCardAction>;
}
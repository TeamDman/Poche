use crate::actions::place_bet_action::BetAction;
use crate::actions::play_card_action::PlayCardAction;
use crate::cards::Card;
use crate::money::MoneyJar;
use crate::policies::policy::Policy;
use crate::random::RandomState;
use crate::state::State;
use eyre::bail;
use std::ops::{Deref, IndexMut};
use std::ops::DerefMut;
use std::ops::Index;
use std::rc::Rc;

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Players {
    /// Clockwise-ordered players
    pub players: Vec<Player>,
    pub dealer_id: Option<PlayerId>,
    pub active_player_id: Option<PlayerId>,
}

impl Index<&PlayerId> for Players {
    type Output = Player;
    fn index(&self, id: &PlayerId) -> &Self::Output {
        self.players
            .iter()
            .find(|player| player.id == *id)
            .unwrap_or_else(|| panic!("Player with id {} not found", id))
    }
}
impl IndexMut<&PlayerId> for Players {
    fn index_mut(&mut self, id: &PlayerId) -> &mut Self::Output {
        self.players
            .iter_mut()
            .find(|player| player.id == *id)
            .unwrap_or_else(|| panic!("Player with id {} not found", id))
    }
}
impl Players {
    pub fn iter_dealer_last(&self) -> eyre::Result<impl Iterator<Item = &Player>> {
        let Some(dealer_id) = &self.dealer_id else {
            bail!("Dealer not set")
        };
        let dealer_position = self.players.iter().position(|player| player.id == *dealer_id).unwrap();
        Ok(self
            .players
            .iter()
            .cycle()
            .skip(dealer_position + 1)
            .take(self.players.len()))
    }
    pub fn get_active_player(&self) -> eyre::Result<(&PlayerId, &Player)> {
        let Some(active_player_id) = &self.active_player_id else {
            bail!("Active player not set")
        };
        Ok((active_player_id, &self[active_player_id]))
    }
    pub fn get_active_player_mut(&mut self) -> eyre::Result<&mut Player> {
        let Some(active_player_id) = self.active_player_id.clone() else {
            bail!("Active player not set")
        };
        Ok(&mut self[&active_player_id])
    }
    pub fn get_wrapped(&self, index: isize) -> (&PlayerId, &Player) {
        let len = self.players.len() as isize;
        let wrapped_index = index.rem_euclid(len) as usize;
        let player_id = &self.players[wrapped_index].id;
        (player_id, &self.players[wrapped_index])
    }
    // pub fn get_player_mut(&mut self, index: isize) -> Option<&mut Player> {
    //     let len = self.players.len() as isize;
    //     let wrapped_index = index.rem_euclid(len) as usize;
    //     self.players.get_mut(wrapped_index)
    // }
    pub fn advance_active_player(&mut self) -> eyre::Result<()> {
        let (active_player_id, _) = self.get_active_player()?;
        let active_player_index = self.players.iter().position(|player| &player.id == active_player_id).unwrap();
        self.active_player_id = Some(self.get_wrapped(active_player_index as isize + 1).0.clone());
        Ok(())
    }

    pub fn set_active_player_to_left_of_dealer(&mut self) -> eyre::Result<()> {
        let Some(dealer_id) = &self.dealer_id else {
            bail!("Dealer not set");
        };
        let dealer_index = self.players.iter().position(|player| &player.id == dealer_id).unwrap();
        let left_of_dealer_index = (dealer_index + 1) % self.players.len();
        self.active_player_id = Some(self.players[left_of_dealer_index].id.clone());
        Ok(())
    }
}
impl Deref for Players {
    type Target = Vec<Player>;
    fn deref(&self) -> &Self::Target {
        &self.players
    }
}
impl DerefMut for Players {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.players
    }
}

/// A unique identifier for a player, cheap to clone
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct PlayerId(pub Rc<str>);

impl PlayerId {
    pub fn new(name: String) -> PlayerId {
        PlayerId(Rc::from(name))
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub hand: Vec<Card>,
    pub tricks: Vec<Vec<Card>>,
    pub score: u32,
    pub money_jar: MoneyJar,
    pub policy: Policy,
    pub bet: Option<u32>,
}
impl Player {
    pub fn new(id: PlayerId, policy: Policy) -> Player {
        Player {
            id,
            policy,
            hand: Vec::new(),
            tricks: Vec::new(),
            score: 0,
            bet: None,
            money_jar: MoneyJar::new(10_000),
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
        self.policy
            .get_behaviour()
            .place_bet(self, rand, choices, state)
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
        self.policy
            .get_behaviour()
            .play_card(self, rand, choices, state)
    }
}


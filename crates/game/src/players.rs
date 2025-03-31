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
    /// Index plus one gives the left player, index minus one gives the right player
    /// 
    /// ```txt
    ///          _________
    ///         /         \
    ///    0   /           \   1
    ///       /             \
    ///      |               |
    ///   5  |     TABLE     |  2
    ///       \             /
    ///        \           /
    ///    4    \_________/    3
    /// ```
    /// 
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
impl Index<isize> for Players {
    type Output = Player;
    fn index(&self, index: isize) -> &Self::Output {
        let len = self.players.len() as isize;
        let wrapped_index = index.rem_euclid(len) as usize;
        &self.players[wrapped_index]
    }
}
impl Index<usize> for Players {
    type Output = Player;
    fn index(&self, index: usize) -> &Self::Output {
        let len = self.players.len();
        let wrapped_index = index.rem_euclid(len);
        &self.players[wrapped_index]
    }
}
pub enum Offset {
    LeftOf(PlayerId),
    RightOf(PlayerId),
}
impl Index<Offset> for Players {
    type Output = Player;
    fn index(&self, offset: Offset) -> &Self::Output {
        match offset {
            Offset::LeftOf(player_id) => {
                let player_index = self.index_for(&player_id).unwrap();
                &self[player_index + 1]
            }
            Offset::RightOf(player_id) => {
                let player_index = self.index_for(&player_id).unwrap() as isize;
                &self[player_index - 1]
            }
        }
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
    pub fn index_for(&self, player_id: &PlayerId) -> eyre::Result<usize> {
        let Some(index) = self.players.iter().position(|player| player.id == *player_id) else {
            bail!("Player with id {} not found", player_id)
        };
        Ok(index)
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
    pub fn advance_active_player(&mut self) -> eyre::Result<()> {
        let (active_player_id, _) = self.get_active_player()?;
        let active_player_index = self.index_for(active_player_id)?;
        self.active_player_id = Some(self[active_player_index + 1].id.clone());
        Ok(())
    }

    pub fn set_active_player_to_left_of_dealer(&mut self) -> eyre::Result<()> {
        let Some(dealer_id) = &self.dealer_id else {
            bail!("Dealer not set");
        };
        let dealer_index = self.index_for(dealer_id)?;
        self.active_player_id = Some(self[dealer_index + 1].id.clone());
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

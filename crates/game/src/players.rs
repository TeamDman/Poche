use crate::cards::Card;
use crate::money::MoneyJar;
use crate::policies::policy::Policy;
use eyre::bail;
use std::ops::Deref;
use std::ops::DerefMut;

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Players {
    /// Clockwise-ordered players
    pub players: Vec<Player>,
    pub dealer_index: Option<usize>,
    pub active_player_index: Option<usize>,
}

impl Players {
    pub fn iter_dealer_last(&self) -> eyre::Result<impl Iterator<Item = (usize, &Player)>> {
        let Some(dealer_index) = self.dealer_index else {
            bail!("Dealer not set")
        };
        Ok(self
            .players
            .iter()
            .enumerate()
            .cycle()
            .skip(dealer_index + 1)
            .take(self.players.len()))
    }
    pub fn get_active_player(&self) -> eyre::Result<(usize, &Player)> {
        let Some(active_player_index) = self.active_player_index else {
            bail!("Active player not set")
        };
        Ok((active_player_index, &self.players[active_player_index]))
    }
    pub fn get_active_player_mut(&mut self) -> eyre::Result<(usize, &mut Player)> {
        let Some(active_player_index) = self.active_player_index else {
            bail!("Active player not set")
        };
        Ok((active_player_index, &mut self.players[active_player_index]))
    }
    pub fn get_wrapped(&self, index: isize) -> (usize, &Player) {
        let len = self.players.len() as isize;
        let wrapped_index = index.rem_euclid(len) as usize;
        (wrapped_index, &self.players[wrapped_index])
    }
    // pub fn get_player_mut(&mut self, index: isize) -> Option<&mut Player> {
    //     let len = self.players.len() as isize;
    //     let wrapped_index = index.rem_euclid(len) as usize;
    //     self.players.get_mut(wrapped_index)
    // }
    pub fn advance_active_player(&mut self) -> eyre::Result<()> {
        let (active_player_index, _) = self.get_active_player()?;
        self.active_player_index = Some(self.get_wrapped(active_player_index as isize + 1).0);
        Ok(())
    }

    pub fn set_active_player_to_left_of_dealer(&mut self) -> eyre::Result<()> {
        let Some(dealer_index) = self.dealer_index else {
            bail!("Dealer not set");
        };
        let left_of_dealer_index = (dealer_index + 1) % self.players.len();
        self.active_player_index = Some(left_of_dealer_index);
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

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct PlayerId(pub String);

impl PlayerId {
    pub fn new(name: String) -> PlayerId {
        PlayerId(name)
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
}

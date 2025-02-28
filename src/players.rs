use std::ops::Deref;
use std::ops::DerefMut;
use crate::cards::Card;
use crate::money::MoneyJar;
use crate::policy::Policy;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Players {
    /// Clockwise-ordered players
    pub players: Vec<Player>,
    pub dealer_index: usize,
}
impl Players {
    pub fn iter_dealer_last(&self) -> impl Iterator<Item = (usize, &Player)> {
        self.players
            .iter()
            .enumerate()
            .cycle()
            .skip(self.dealer_index + 1)
            .take(self.players.len())
    }
    pub fn get_left_of_dealer(&self) -> (usize, &Player) {
        let left_of_dealer_index = self.dealer_index + 1 % self.players.len();
        (left_of_dealer_index, &self.players[left_of_dealer_index])
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
    pub points: u32,
    pub money_jar: MoneyJar,
    pub policy: Policy,
}

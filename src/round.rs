#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Direction {
    Up,
    Down,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Round {
    pub round_number: u32,
    pub hand_size: u32,
    pub direction: Direction,
}

impl Round {
    pub(crate) fn reset(&mut self) {
        *self = Round::default();
    }
}

impl Default for Round {
    fn default() -> Self {
        Round {
            round_number: 1,
            hand_size: 1,
            direction: Direction::Up,
        }
    }
}
impl Round {
    /// Start at hand_size 1, go up once each round until not enough cards, then go down
    /// 1,2,3,4,5,6,7,6,5,4,3,2,1,finished
    pub fn try_advance(&mut self, num_players: u32) -> eyre::Result<()> {
        if self.hand_size == 1 && self.direction == Direction::Down {
            return Err(eyre::eyre!("The game is already finished, cannot advance to next round"));
        }
        self.round_number += 1;
        if self.round_number > 7 {
            self.direction = Direction::Down;
        }
        if (num_players * self.hand_size + 1) > 52 {
            self.direction = Direction::Down;
        }
        self.hand_size = match self.direction {
            Direction::Up => self.hand_size + 1,
            Direction::Down => self.hand_size - 1,
        };
        Ok(())
    }
    pub fn is_last_round(&self) -> bool {
        self.hand_size == 1 && self.direction == Direction::Up
    }
}
#[cfg(test)]
mod tests {
    use crate::round::Round;

    #[test]
    fn it_works() {
        let mut round = Round::default();
        let num_players = 4;
    }
}

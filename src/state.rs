use color_eyre::owo_colors::OwoColorize;
use eyre::bail;
use crate::action::Action;
use crate::cards::{Deck, Suit};
use crate::money::Coin;
use crate::money::MoneyJar;
use crate::players::Player;
use crate::players::PlayerId;
use crate::players::Players;
use crate::policy::Policy;
use crate::random::RandomState;
use crate::round::Round;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct State {
    pub deck: Deck,
    pub players: Players,
    pub pot: MoneyJar,
    pub timestep: u32,
    pub rand: RandomState,
    pub pile: Deck,
    pub round: Round,
}

impl Default for State {
    fn default() -> Self {
        let mut rand = RandomState::new();

        let mut players = Vec::new();
        for x in 0..4 {
            let player_id = PlayerId::new(format!("Player {}", x));
            let money_jar = MoneyJar::from([Coin::Dime, Coin::Quarter].repeat(5));
            let player = Player {
                id: player_id,
                hand: Vec::new(),
                points: 0,
                money_jar,
                policy: Policy::Random,
            };
            players.push(player);
        }
        
        let mut pot = Default::default();
        for player in players.iter_mut() {
            player.money_jar -= MoneyJar::from(vec![Coin::Quarter]);
            pot += MoneyJar::from(vec![Coin::Quarter]);
        }

        let dealer_index = rand.gen_range(0..players.len());
        let active_player_index = dealer_index + 1 % players.len();
        let players = Players {
            dealer_index,
            active_player_index,
            players,
        };
        
        let mut state = State {
            players,
            deck: Deck::new_full(),
            pot,
            timestep: 0,
            rand,
            pile: Deck::new_empty(),
            round: Round::default(),
        };
        
        state.deal_until_everyone_has_n_cards(1).unwrap();
        
        state
    }
}

impl State {
    pub fn step(&mut self) -> eyre::Result<Action> {
        if self.is_done() {
            bail!("State is done");
        }
        let actions = Action::get_valid_actions(&self)?;
        assert!(actions.len() > 0);
        let active_player_index = self.players.active_player_index;
        let active_player = &mut self.players[active_player_index];
        let action = active_player.policy.pick_action(&mut self.rand, actions);
        action.apply(self);
        
        // advance to the next player's turn
        self.players.active_player_index = (self.players.active_player_index + 1) % self.players.len();

        if self.players.iter().all(|p| p.hand.is_empty()) {
            self.end_round()?;
        }
        Ok(action)
    }
    
    pub(crate) fn get_trump(&self) -> Option<Suit> {
        self.deck.cards.last().map(|c| c.suit)
    }
    pub fn is_done(&self) -> bool {
        self.round.is_last_round() && self.players.iter().all(|p| p.hand.is_empty())
    }
    
    pub(crate) fn get_suit_to_follow(&self) -> Option<Suit> {
        self.pile.cards.first().map(|c| c.suit)
    }

    fn shuffle_deck(&mut self) {
        self.rand.shuffle(&mut self.deck.cards);
    }

    fn deal_until_everyone_has_n_cards(&mut self, n: u32) -> eyre::Result<()> {
        loop {
            let mut next_player_to_receive_card = None;
            for (i, player) in self.players.iter_dealer_last() {
                if player.hand.len() < n as usize {
                    next_player_to_receive_card = Some(i);
                    break;
                }
            }
            match next_player_to_receive_card {
                Some(i) => {
                    let card = self.deck.take_top_card()?;
                    self.players[i].hand.push(card);
                }
                None => break,
            }
        }
        Ok(())
    }

    fn end_round(&mut self) -> eyre::Result<()> {
        println!("Ending round");
        if self.is_done() {
            println!("The last round has ended!");
            self.round.reset();
            return Ok(()); // the game is over
        } else {
            self.round.try_advance(self.players.len() as u32)?;
        }
        self.deck.cards.extend(self.pile.cards.drain(..));
        self.shuffle_deck();
        self.players.dealer_index = (self.players.dealer_index + 1) % self.players.len();
        self.deal_until_everyone_has_n_cards(self.round.hand_size)?;
        Ok(())
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Poche Game\n")?;
        f.write_str("Players:\n")?;
        for player in self.players.iter() {
            f.write_fmt(format_args!(
                "  {} has {} points, {} cards in hand, and {} in their cash jar\n",
                player.id.0,
                player.points,
                player.hand.len(),
                player.money_jar
            ))?;
        }
        f.write_fmt(format_args!(
            "The dealer is {}, the active player is {}, and the pot is {}\n",
            self.players.dealer_index,
            self.players.active_player_index,
            self.pot,
        ))?;
        f.write_fmt(format_args!(
            "Trump is {:?}\n",
            self.get_trump(),
        ))?;
        f.write_fmt(format_args!(
            "Follow-suit is {:?}\n",
            self.get_suit_to_follow()
        ))?;
        f.write_fmt(format_args!(
            "The deck has {} cards left\n",
            self.deck.cards.len()
        ))?;
        f.write_fmt(format_args!(
            "The pile has {} cards\n",
            self.pile.cards.len()
        ))?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::state::State;

    #[test]
    pub fn bruh() {
        println!("{}", 2 - 6 % 10);
    }
    #[test]
    pub fn left_of_dealer() {
        let state = State::default();
    }
}

use crate::action::Action;
use crate::cards::Card;
use crate::cards::Deck;
use crate::cards::Suit;
use crate::money::Coin;
use crate::money::MoneyJar;
use crate::players::Player;
use crate::players::PlayerId;
use crate::players::Players;
use crate::policy::Policy;
use crate::random::RandomState;
use crate::round::Round;
use crate::rule::Rule;
use eyre::OptionExt;
use eyre::bail;
use std::collections::HashSet;
use std::collections::VecDeque;

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct State {
    pub deck: Deck,
    pub players: Players,
    pub pot: MoneyJar,
    pub timestep: u32,
    pub rand: RandomState,
    pub pile: Deck,
    pub round: Round,
    pub stack: VecDeque<Rule>,
}

impl Default for State {
    fn default() -> Self {
        State::new(4)
    }
}

impl State {
    pub fn new(player_count: usize) -> State {
        let mut rand = RandomState::new();

        let mut players = Vec::new();
        for x in 0..player_count {
            let player_id = PlayerId::new(format!("Player {}", x));
            let player = Player::new(player_id, Policy::Random);
            players.push(player);
        }

        let mut pot = Default::default();
        for player in players.iter_mut() {
            player.money_jar -= MoneyJar::from(vec![Coin::Quarter]);
            pot += MoneyJar::from(vec![Coin::Quarter]);
        }

        let dealer_index = rand.gen_range(0..players.len());
        let active_player_index = (dealer_index + 1) % players.len();
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
            stack: Default::default(),
        };

        state.deal_until_everyone_has_n_cards(1).unwrap();

        state
    }

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

        // If all the players have the same number of cards in hand again, then everyone has played their card
        let trick_finished = self
            .players
            .iter()
            .map(|p| p.hand.len())
            .collect::<HashSet<_>>()
            .len()
            == 1;
        if trick_finished {
            self.handle_trick_finished()?;
        } else {
            // advance to the next player's turn
            self.players.active_player_index =
                (self.players.active_player_index + 1) % self.players.len();
        }

        if self.players.iter().all(|p| p.hand.is_empty()) {
            self.end_round()?;
        }
        Ok(action)
    }

    pub fn handle_trick_finished(&mut self) -> eyre::Result<()> {
        // determine the highest card played
        let Some(trump) = self.get_trump() else {
            bail!("Trump suit missing, how did we get here?");
        };
        let Some(follow_suit) = self.get_suit_to_follow() else {
            bail!("Follow suit missing, how did we get here?");
        };

        println!(
            "Determining winner of the trick. {} was lead, {} was trump.\n========",
            follow_suit, trump
        );
        let mut played = self.get_played_cards();
        for (card, player) in &played {
            println!("{} played {}", player.id, card);
        }
        println!("========");
        played.sort_by(|a, b| {
            a.0.value(follow_suit, trump)
                .cmp(&b.0.value(follow_suit, trump))
        });
        let winner = played
            .into_iter()
            .next()
            .ok_or_eyre("Could not find winner")?;
        println!("{} won the trick with the {}", winner.1.id, winner.0);
        self.players.active_player_index = self
            .players
            .iter()
            .position(|p| p.id == winner.1.id)
            .ok_or_eyre("Could not find active player")?;
        Ok(())
    }

    pub fn get_trump(&self) -> Option<Suit> {
        self.deck.last().map(|c| c.suit)
    }
    pub fn is_done(&self) -> bool {
        self.round.is_last_round() && self.players.iter().all(|p| p.hand.is_empty())
    }

    pub fn get_suit_to_follow(&self) -> Option<Suit> {
        self.pile.first().map(|c| c.suit)
    }

    pub fn get_played_cards(&self) -> Vec<(Card, &Player)> {
        // The most recent card was played by the active player
        // We can walk backwards to find who played each card
        let mut rtn = Vec::new();
        let mut played_cards = self.pile.clone();
        let mut player_index = self.players.active_player_index;
        while let Some(card) = played_cards.pop() {
            let player = &self.players[player_index];
            rtn.push((card, player));
            player_index = match player_index {
                0 => self.players.len() - 1,
                _ => player_index - 1,
            }
        }
        rtn
    }

    pub fn shuffle_deck(&mut self) {
        self.rand.shuffle(&mut self.deck);
    }

    pub fn deal_until_everyone_has_n_cards(&mut self, n: u32) -> eyre::Result<()> {
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
                    let card = (&mut self.deck)
                        .pop()
                        .ok_or_eyre("Tried to deal when no cards remaining")?;
                    self.players[i].hand.push(card);
                }
                None => break,
            }
        }
        Ok(())
    }

    pub fn end_round(&mut self) -> eyre::Result<()> {
        println!("======================== Ending round ================================");
        if self.is_done() {
            println!("The last round has ended!");
            // self.round.reset();
            return Ok(()); // the game is over
        } else {
            self.round.try_advance(self.players.len() as u32)?;
        }
        self.deck.extend(self.pile.drain(..));
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
            self.players.dealer_index, self.players.active_player_index, self.pot,
        ))?;
        f.write_fmt(format_args!("Trump is {:?}\n", self.get_trump(),))?;
        f.write_fmt(format_args!(
            "Follow-suit is {:?}\n",
            self.get_suit_to_follow()
        ))?;
        f.write_fmt(format_args!(
            "The deck has {} cards left\n",
            self.deck.len()
        ))?;
        f.write_fmt(format_args!("The pile has {} cards\n", self.pile.len()))?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    #[test]
    pub fn bruh() {
        println!("{}", 2 - 6 % 10);
    }
}

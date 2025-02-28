use color_eyre::owo_colors::OwoColorize;
use crate::action::Action;
use crate::cards::Deck;
use crate::money::Coin;
use crate::money::MoneyJar;
use crate::players::Player;
use crate::players::PlayerId;
use crate::players::Players;
use crate::policy::Policy;
use crate::random::RandomState;
use crate::round::Round;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GamePhase {
    TakingBuyIn,
    Playing {
        round: Round,
        active_player_index: usize,
    },
    GameOver,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct State {
    pub deck: Deck,
    pub players: Players,
    pub pot: MoneyJar,
    pub timestep: u32,
    pub phase: GamePhase,
    pub rand: RandomState,
    pub pile: Deck,
}

impl State {
    pub fn is_done(&self) -> bool {
        matches!(self.phase, GamePhase::GameOver) 
    }
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

        let players = Players {
            dealer_index: rand.gen_range(0..players.len()),
            players,
        };

        let state = State {
            players,
            deck: Deck::new_full(),
            pot: Default::default(),
            timestep: 0,
            rand,
            phase: GamePhase::TakingBuyIn,
            pile: Deck::new_empty(),
        };
        state
    }
}

impl State {
    pub fn advance(&mut self) -> eyre::Result<Option<Action>> {
        let mut action_taken = None;
        let next_phase = match self.phase.clone() {
            GamePhase::TakingBuyIn => {
                for player in self.players.iter_mut() {
                    player.money_jar -= MoneyJar::from(vec![Coin::Quarter]);
                    self.pot += MoneyJar::from(vec![Coin::Quarter]);
                }
                self.deal_until_everyone_has_n_cards(1)?;
                GamePhase::Playing {
                    round: Default::default(),
                    active_player_index: self.players.get_left_of_dealer().0,
                }
            }
            GamePhase::Playing {
                active_player_index,
                mut round,
            } => {
                let actions = Action::get_valid_actions(&self)?;
                assert!(actions.len() > 0);
                let phase = match round.try_advance(self.players.len() as u32) {
                    Ok(()) => {
                        self.deal_until_everyone_has_n_cards(round.hand_size)?;
                        GamePhase::Playing {
                            active_player_index: (active_player_index + 1) % self.players.len(),
                            round,
                        }
                    },
                    Err(_) => {
                        GamePhase::GameOver
                    }
                };
                
                let active_player = &mut self.players[active_player_index];
                let action = active_player.policy.pick_action(&mut self.rand, actions);
                action.apply(self);
                action_taken = Some(action);
                if self.players.iter().all(|p| p.hand.is_empty()) {
                    self.end_round();
                }
                phase
            }
            GamePhase::GameOver => GamePhase::GameOver,
        };
        self.phase = next_phase;
        
        Ok(action_taken)
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

    fn end_round(&mut self) {
        self.deck.cards.extend(self.pile.cards.drain(..));
        self.shuffle_deck();
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Poche Game\n")?;
        f.write_str("Players:\n")?;
        for player in self.players.iter() {
            f.write_fmt(format_args!(
                "  {} has {} points and {} in their cash jar\n",
                player.id.0, player.points, player.money_jar
            ))?;
        }
        f.write_fmt(format_args!(
            "The dealer is {} and the pot is {}\n",
            self.players.get_left_of_dealer().1.id,
            self.pot,
        ))?;
        f.write_fmt(format_args!(
            "The deck has {} cards left\n",
            self.deck.cards.len()
        ))?;
        f.write_fmt(format_args!(
            "The pile has {} cards\n",
            self.pile.cards.len()
        ))?;
        f.write_fmt(format_args!("Phase: {:?}", self.phase))?;
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

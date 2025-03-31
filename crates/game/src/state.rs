use crate::cards::Card;
use crate::cards::Deck;
use crate::cards::Suit;
use crate::money::MoneyJar;
use crate::players::Player;
use crate::players::PlayerId;
use crate::players::Players;
use crate::policies::policy::Policy;
use crate::random::RandomState;
use crate::round::Round;
use crate::rules::assertions::assert_invariants;
use crate::rules::rule::Rule;
use crate::rules::rule::RuleBehaviour;
use std::collections::VecDeque;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct State {
    pub deck: Deck,
    pub players: Players,
    pub pot: MoneyJar,
    pub timestep: u32,
    pub rand: RandomState,
    pub pile: Vec<Card>,
    pub round: Round,
    pub stack: VecDeque<Rule>,
    pub trump: Option<Card>,
}

impl State {
    pub fn new_with_random_players(player_count: usize) -> State {
        let mut state = State {
            deck: Deck::new_full(),
            players: Default::default(),
            pot: Default::default(),
            timestep: Default::default(),
            rand: Default::default(),
            pile: Default::default(),
            round: Default::default(),
            stack: Default::default(),
            trump: Default::default(),
        };
        for x in 0..player_count {
            let player_id = PlayerId::new(format!("Player {}", x));
            let player = Player::new(player_id, Policy::Random);
            state.players.push(player);
        }
        state.stack.push_front(Rule::Ante);
        assert_invariants(&state);
        state
    }
    pub fn tick(&mut self) -> eyre::Result<Option<Rule>> {
        let Some(rule) = self.stack.pop_front() else {
            return Ok(None);
        };
        assert_invariants(self);
        println!("Applying rule {:?} (stack is now {:?})", rule, self.stack);
        rule.apply(self)?;
        assert_invariants(self);
        Ok(Some(rule))
    }

    pub fn get_trump(&self) -> Option<Suit> {
        self.deck.last().map(|c| c.suit)
    }
    // pub fn is_done(&self) -> bool {
    //     self.round.is_last_round() && self.players.iter().all(|p| p.hand.is_empty())
    // }

    pub fn get_suit_to_follow(&self) -> Option<Suit> {
        self.pile.first().map(|c| c.suit)
    }

    pub fn get_played_cards(&self) -> Vec<(Card, usize, &Player)> {
        // The most recent card was played by the active player
        // We can walk backwards to find who played each card
        let mut rtn = Vec::new();
        let mut played_cards = self.pile.clone();
        let mut player_index = self.players.active_player_index.unwrap();
        while let Some(card) = played_cards.pop() {
            let player = &self.players[player_index];
            rtn.push((card, player_index, player));
            player_index = match player_index {
                0 => self.players.len() - 1,
                _ => player_index - 1,
            }
        }
        rtn
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("=== Poche Game State ===\n")?;

        // Write scores
        f.write_str("Scores: ")?;
        for player in self.players.iter() {
            f.write_fmt(format_args!("{}: {}\t", player.id.0, player.score))?;
        }
        f.write_str("\n")?;

        // Write pot
        f.write_fmt(format_args!("Pot: {}\n", self.pot))?;

        // Write card counts
        f.write_str("Card counts: ")?;
        f.write_fmt(format_args!("Deck: {}\t", self.deck.len()))?;
        f.write_fmt(format_args!("Pile: {}\t", self.pile.len()))?;
        for player in self.players.iter() {
            f.write_fmt(format_args!(
                "{}: hand={}, tricks={}\t",
                player.id.0,
                player.hand.len(),
                player.tricks.iter().map(|t| t.len()).sum::<usize>()
            ))?;
        }
        f.write_str("\n")?;

        // Write round info
        f.write_fmt(format_args!("Round: {}\n", self.round))?;

        // Write player info
        f.write_fmt(format_args!("Dealer: {:?}\n", self.players.dealer_index))?;
        f.write_fmt(format_args!(
            "Active player: {:?}\n",
            self.players.active_player_index
        ))?;

        // Write trump and suit
        f.write_fmt(format_args!("Trump: {:?}\n", self.trump))?;
        f.write_fmt(format_args!(
            "Suit to follow: {:?}\n",
            self.get_suit_to_follow()
        ))?;

        // Write stack
        f.write_fmt(format_args!("Stack: {:?}", self.stack))?;

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

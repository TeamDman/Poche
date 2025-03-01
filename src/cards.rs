use std::fmt::Formatter;
use std::ops::Deref;
use std::ops::DerefMut;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}
impl std::fmt::Display for Suit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}
impl std::fmt::Display for Rank {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
impl Rank {
    pub fn value(&self) -> usize {
        match self {
            Rank::Two => 2,
            Rank::Three => 3,
            Rank::Four => 4,
            Rank::Five => 5,
            Rank::Six => 6,
            Rank::Seven => 7,
            Rank::Eight => 8,
            Rank::Nine => 9,
            Rank::Ten => 10,
            Rank::Jack => 11,
            Rank::Queen => 12,
            Rank::King => 13,
            Rank::Ace => 14,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}
impl Card {
    pub fn value(&self, follow_suit: Suit, trump: Suit) -> usize {
        let base_value = self.rank.value();
        match self.suit {
            s if s == trump => base_value + 200,
            s if s == follow_suit => base_value + 100,
            _ => base_value,
        }
    }
}
impl std::fmt::Display for Card {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} of {}", self.rank, self.suit))
    }
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn len(&self) -> usize {
        self.cards.len()
    }
}

impl Deck {
    pub fn new_full() -> Deck {
        let mut cards = Vec::new();
        for suit in [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs].iter() {
            for rank in [
                Rank::Two,
                Rank::Three,
                Rank::Four,
                Rank::Five,
                Rank::Six,
                Rank::Seven,
                Rank::Eight,
                Rank::Nine,
                Rank::Ten,
                Rank::Jack,
                Rank::Queen,
                Rank::King,
                Rank::Ace,
            ]
            .iter()
            {
                cards.push(Card {
                    suit: *suit,
                    rank: *rank,
                });
            }
        }
        Deck { cards }
    }
    pub fn new_empty() -> Deck {
        Deck { cards: Vec::new() }
    }
}

impl Deref for Deck {
    type Target = Vec<Card>;
    fn deref(&self) -> &Self::Target {
        &self.cards
    }
}
impl DerefMut for Deck {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cards
    }
}

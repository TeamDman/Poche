use eyre::OptionExt;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
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
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Deck {
    pub cards: Vec<Card>,
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
    pub fn take_top_card(&mut self) -> eyre::Result<Card> {
        self.cards.pop().ok_or_eyre("Tried to draw when no cards remaining")
    }
    pub fn push(&mut self, card: Card) {
        self.cards.push(card);
    }
}
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Conventional, independently authored executable oracle for Poche.
//!
//! This model follows the rule IDs in `docs/rules-coverage.md`. It deliberately
//! does not use the later Weavy formal subset, so agreement between the two Rust
//! implementations can expose lowering or authoring defects.

use std::array;

pub const DECK_SIZE: usize = 52;
pub const MAX_HAND_SIZE: usize = 7;
pub const OPENING_ANTE_CENTS: u32 = 25;
pub const MISSED_BID_PAYMENT_CENTS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Self; 4] = [Self::Clubs, Self::Diamonds, Self::Hearts, Self::Spades];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Rank {
    pub const ALL: [Self; 13] = [
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
        Self::Ace,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

impl Card {
    #[must_use]
    pub const fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }

    #[must_use]
    pub fn standard_deck() -> [Self; DECK_SIZE] {
        array::from_fn(|index| {
            let suit = Suit::ALL[index / Rank::ALL.len()];
            let rank = Rank::ALL[index % Rank::ALL.len()];
            Self { suit, rank }
        })
    }

    const fn index(self) -> usize {
        (self.suit as usize) * Rank::ALL.len() + self.rank as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedCards<const CAPACITY: usize> {
    slots: [Option<Card>; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> Default for FixedCards<CAPACITY> {
    fn default() -> Self {
        Self {
            slots: [None; CAPACITY],
            len: 0,
        }
    }
}

impl<const CAPACITY: usize> FixedCards<CAPACITY> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = Card> + '_ {
        self.slots.iter().flatten().copied()
    }

    #[must_use]
    pub fn contains(&self, card: Card) -> bool {
        self.iter().any(|candidate| candidate == card)
    }

    #[must_use]
    pub fn contains_suit(&self, suit: Suit) -> bool {
        self.iter().any(|card| card.suit == suit)
    }

    fn push(&mut self, card: Card) -> Result<(), RuleViolation> {
        let Some(slot) = self.slots.iter_mut().find(|slot| slot.is_none()) else {
            return Err(RuleViolation::CardZoneFull);
        };
        *slot = Some(card);
        self.len += 1;
        Ok(())
    }

    fn remove(&mut self, card: Card) -> Result<(), RuleViolation> {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.is_some_and(|candidate| candidate == card))
        else {
            return Err(RuleViolation::CardNotInHand(card));
        };
        *slot = None;
        self.len -= 1;
        Ok(())
    }
}

pub type Hand = FixedCards<MAX_HAND_SIZE>;
pub type CardPile = FixedCards<DECK_SIZE>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seat<const PLAYERS: usize>(usize);

impl<const PLAYERS: usize> Seat<PLAYERS> {
    /// Creates a seat in a statically sized game.
    ///
    /// # Errors
    ///
    /// Returns [`RuleViolation::InvalidPlayerCount`] when `PLAYERS` is outside
    /// 2 through 51, or [`RuleViolation::InvalidSeat`] when `index` is outside
    /// the array.
    pub fn new(index: usize) -> Result<Self, RuleViolation> {
        validate_player_count(PLAYERS)?;
        if index < PLAYERS {
            Ok(Self(index))
        } else {
            Err(RuleViolation::InvalidSeat {
                index,
                players: PLAYERS,
            })
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }

    #[must_use]
    pub fn left(self) -> Self {
        Self((self.0 + 1) % PLAYERS)
    }

    #[must_use]
    pub fn advance(self, count: usize) -> Self {
        Self((self.0 + count) % PLAYERS)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckOrder([Card; DECK_SIZE]);

impl DeckOrder {
    /// Validates a candidate ordering of the standard 52-card deck.
    ///
    /// # Errors
    ///
    /// Returns [`RuleViolation::DuplicateCard`] or
    /// [`RuleViolation::IncompleteDeck`] unless every standard card occurs
    /// exactly once.
    pub fn new(cards: [Card; DECK_SIZE]) -> Result<Self, RuleViolation> {
        let mut seen = [false; DECK_SIZE];
        for card in cards {
            let index = card.index();
            if seen[index] {
                return Err(RuleViolation::DuplicateCard(card));
            }
            seen[index] = true;
        }
        if seen.iter().any(|present| !present) {
            return Err(RuleViolation::IncompleteDeck);
        }
        Ok(Self(cards))
    }

    #[must_use]
    pub fn standard() -> Self {
        Self(Card::standard_deck())
    }

    #[must_use]
    pub const fn cards(&self) -> &[Card; DECK_SIZE] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseTag {
    AwaitingDeal,
    Bidding,
    Playing,
    Scoring,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn<const PLAYERS: usize> {
    Chance,
    Player(Seat<PLAYERS>),
    Environment,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ledger<const PLAYERS: usize> {
    pub dealer: Seat<PLAYERS>,
    pub round_index: usize,
    pub scores: [u16; PLAYERS],
    pub pot_cents: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwaitingDeal<const PLAYERS: usize> {
    pub ledger: Ledger<PLAYERS>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bidding<const PLAYERS: usize> {
    pub ledger: Ledger<PLAYERS>,
    pub hand_size: u8,
    pub hands: [Hand; PLAYERS],
    pub trump: Card,
    pub stock: CardPile,
    pub bids: [Option<u8>; PLAYERS],
    pub actor: Seat<PLAYERS>,
    bids_made: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayedCard<const PLAYERS: usize> {
    pub player: Seat<PLAYERS>,
    pub card: Card,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trick<const PLAYERS: usize> {
    cards: [Option<PlayedCard<PLAYERS>>; PLAYERS],
    len: usize,
}

impl<const PLAYERS: usize> Default for Trick<PLAYERS> {
    fn default() -> Self {
        Self {
            cards: [None; PLAYERS],
            len: 0,
        }
    }
}

impl<const PLAYERS: usize> Trick<PLAYERS> {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = PlayedCard<PLAYERS>> + '_ {
        self.cards.iter().flatten().copied()
    }

    #[must_use]
    pub fn lead_suit(&self) -> Option<Suit> {
        self.cards
            .first()
            .and_then(|played| played.map(|item| item.card.suit))
    }

    #[must_use]
    pub fn winner(&self, trump: Suit) -> Option<Seat<PLAYERS>> {
        if self.len != PLAYERS {
            return None;
        }
        let lead = self.lead_suit()?;
        self.iter()
            .max_by_key(|played| {
                let category = if played.card.suit == trump {
                    2_u8
                } else {
                    u8::from(played.card.suit == lead)
                };
                (category, played.card.rank)
            })
            .map(|played| played.player)
    }

    fn push(&mut self, played: PlayedCard<PLAYERS>) -> Result<(), RuleViolation> {
        if self.len >= PLAYERS {
            return Err(RuleViolation::TrickFull);
        }
        self.cards[self.len] = Some(played);
        self.len += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing<const PLAYERS: usize> {
    pub ledger: Ledger<PLAYERS>,
    pub hand_size: u8,
    pub hands: [Hand; PLAYERS],
    pub trump: Card,
    pub stock: CardPile,
    pub bids: [u8; PLAYERS],
    pub actor: Seat<PLAYERS>,
    pub leader: Seat<PLAYERS>,
    pub trick: Trick<PLAYERS>,
    pub captured: [CardPile; PLAYERS],
    pub tricks_won: [u8; PLAYERS],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scoring<const PLAYERS: usize> {
    pub ledger: Ledger<PLAYERS>,
    pub hand_size: u8,
    pub trump: Card,
    pub stock: CardPile,
    pub bids: [u8; PLAYERS],
    pub captured: [CardPile; PLAYERS],
    pub tricks_won: [u8; PLAYERS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finished<const PLAYERS: usize> {
    pub scores: [u16; PLAYERS],
    pub pot_cents: u32,
    pub winners: [bool; PLAYERS],
}

impl<const PLAYERS: usize> Finished<PLAYERS> {
    #[must_use]
    pub fn pot_division(&self) -> Option<PotDivision> {
        let winner_count = self.winners.iter().filter(|winner| **winner).count();
        let divisor = u32::try_from(winner_count).ok()?;
        if divisor == 0 {
            return None;
        }
        Some(PotDivision {
            winner_count,
            cents_per_winner: self.pot_cents / divisor,
            remainder_cents: self.pot_cents % divisor,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameState<const PLAYERS: usize> {
    AwaitingDeal(AwaitingDeal<PLAYERS>),
    Bidding(Bidding<PLAYERS>),
    Playing(Playing<PLAYERS>),
    Scoring(Scoring<PLAYERS>),
    Finished(Finished<PLAYERS>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game<const PLAYERS: usize> {
    state: GameState<PLAYERS>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action<const PLAYERS: usize> {
    Deal(DeckOrder),
    Bid { player: Seat<PLAYERS>, tricks: u8 },
    Play { player: Seat<PLAYERS>, card: Card },
    SettleRound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BidOutcome {
    Missed,
    Exact,
    AllTricks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundScore {
    pub bid: u8,
    pub tricks: u8,
    pub outcome: BidOutcome,
    pub points: u16,
    pub payment_cents: u32,
}

impl RoundScore {
    #[must_use]
    pub const fn score_cell(self) -> ScoreCell {
        match self.outcome {
            BidOutcome::Missed => ScoreCell::Poche,
            BidOutcome::Exact => ScoreCell::Exact {
                bid: self.bid,
                written: 10 + self.bid,
            },
            BidOutcome::AllTricks => ScoreCell::AllTricks {
                bid: self.bid,
                written: 20 + self.bid,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreCell {
    Poche,
    Exact { bid: u8, written: u8 },
    AllTricks { bid: u8, written: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PotDivision {
    pub winner_count: usize,
    pub cents_per_winner: u32,
    pub remainder_cents: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HighCardDraw<const PLAYERS: usize> {
    pub dealt: [Option<Card>; PLAYERS],
    pub tied: [bool; PLAYERS],
    pub selected: Option<Seat<PLAYERS>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition<const PLAYERS: usize> {
    pub next: Game<PLAYERS>,
    pub round_scores: Option<[RoundScore; PLAYERS]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation<const PLAYERS: usize> {
    pub phase: PhaseTag,
    pub viewer: Seat<PLAYERS>,
    pub dealer: Option<Seat<PLAYERS>>,
    pub actor: Turn<PLAYERS>,
    pub round_index: usize,
    pub hand_size: u8,
    pub private_hand: Hand,
    pub hand_counts: [u8; PLAYERS],
    pub trump: Option<Card>,
    pub current_trick: Trick<PLAYERS>,
    pub bids: [Option<u8>; PLAYERS],
    pub tricks_won: [u8; PLAYERS],
    pub scores: [u16; PLAYERS],
    pub pot_cents: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleViolation {
    InvalidPlayerCount(usize),
    InvalidSeat { index: usize, players: usize },
    DuplicateCard(Card),
    IncompleteDeck,
    CardZoneFull,
    WrongPhase,
    WrongActor,
    BidOutOfRange { bid: u8, hand_size: u8 },
    CardNotInHand(Card),
    MustFollowSuit(Suit),
    TrickFull,
    NoSelectionCandidates,
    CardConservation { card: Card, count: u8 },
}

impl<const PLAYERS: usize> Game<PLAYERS> {
    /// Creates a prepared game after the first dealer has been selected.
    ///
    /// # Errors
    ///
    /// Returns [`RuleViolation::InvalidPlayerCount`] unless `PLAYERS` is in the
    /// rulebook's supported range.
    pub fn new(first_dealer: Seat<PLAYERS>) -> Result<Self, RuleViolation> {
        validate_player_count(PLAYERS)?;
        let player_count =
            u32::try_from(PLAYERS).map_err(|_| RuleViolation::InvalidPlayerCount(PLAYERS))?;
        Ok(Self {
            state: GameState::AwaitingDeal(AwaitingDeal {
                ledger: Ledger {
                    dealer: first_dealer,
                    round_index: 0,
                    scores: [0; PLAYERS],
                    pot_cents: OPENING_ANTE_CENTS * player_count,
                },
            }),
        })
    }

    #[must_use]
    pub const fn state(&self) -> &GameState<PLAYERS> {
        &self.state
    }

    #[must_use]
    pub const fn phase(&self) -> PhaseTag {
        match self.state {
            GameState::AwaitingDeal(_) => PhaseTag::AwaitingDeal,
            GameState::Bidding(_) => PhaseTag::Bidding,
            GameState::Playing(_) => PhaseTag::Playing,
            GameState::Scoring(_) => PhaseTag::Scoring,
            GameState::Finished(_) => PhaseTag::Finished,
        }
    }

    #[must_use]
    pub const fn turn(&self) -> Turn<PLAYERS> {
        match &self.state {
            GameState::AwaitingDeal(_) => Turn::Chance,
            GameState::Bidding(state) => Turn::Player(state.actor),
            GameState::Playing(state) => Turn::Player(state.actor),
            GameState::Scoring(_) => Turn::Environment,
            GameState::Finished(_) => Turn::Finished,
        }
    }

    #[must_use]
    pub fn legal_player_actions(&self) -> Vec<Action<PLAYERS>> {
        match &self.state {
            GameState::Bidding(state) => (0..=state.hand_size)
                .map(|tricks| Action::Bid {
                    player: state.actor,
                    tricks,
                })
                .collect(),
            GameState::Playing(state) => legal_card_actions(state),
            _ => Vec::new(),
        }
    }

    /// Applies one explicit chance, player, or environment action.
    ///
    /// # Errors
    ///
    /// Returns a [`RuleViolation`] when the action is unavailable in the
    /// current phase, has the wrong actor/value, violates follow-suit, or would
    /// break a card-zone invariant.
    pub fn transition(
        &self,
        action: Action<PLAYERS>,
    ) -> Result<Transition<PLAYERS>, RuleViolation> {
        match (&self.state, action) {
            (GameState::AwaitingDeal(state), Action::Deal(deck)) => deal(state, &deck),
            (GameState::Bidding(state), Action::Bid { player, tricks }) => {
                place_bid(state, player, tricks)
            }
            (GameState::Playing(state), Action::Play { player, card }) => {
                play_card(state, player, card)
            }
            (GameState::Scoring(state), Action::SettleRound) => Ok(settle_round(state)),
            _ => Err(RuleViolation::WrongPhase),
        }
    }

    /// Checks conservation of every standard card in phases that own cards.
    ///
    /// # Errors
    ///
    /// Returns [`RuleViolation::CardConservation`] when any card occurs zero or
    /// multiple times across hands, trump, stock, current trick, and captures.
    pub fn validate(&self) -> Result<(), RuleViolation> {
        match &self.state {
            GameState::Bidding(state) => validate_card_zones(
                &state.hands,
                state.trump,
                &state.stock,
                &Trick::default(),
                &array::from_fn(|_| CardPile::default()),
            ),
            GameState::Playing(state) => validate_card_zones(
                &state.hands,
                state.trump,
                &state.stock,
                &state.trick,
                &state.captured,
            ),
            GameState::Scoring(state) => validate_card_zones(
                &array::from_fn(|_| Hand::default()),
                state.trump,
                &state.stock,
                &Trick::default(),
                &state.captured,
            ),
            GameState::AwaitingDeal(_) | GameState::Finished(_) => Ok(()),
        }
    }

    pub fn observe(&self, viewer: Seat<PLAYERS>) -> Observation<PLAYERS> {
        match &self.state {
            GameState::AwaitingDeal(state) => empty_observation(
                viewer,
                PhaseTag::AwaitingDeal,
                Turn::Chance,
                Some(state.ledger),
            ),
            GameState::Bidding(state) => Observation {
                phase: PhaseTag::Bidding,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: Turn::Player(state.actor),
                round_index: state.ledger.round_index,
                hand_size: state.hand_size,
                private_hand: state.hands[viewer.index()].clone(),
                hand_counts: hand_counts(&state.hands),
                trump: Some(state.trump),
                current_trick: Trick::default(),
                bids: state.bids,
                tricks_won: [0; PLAYERS],
                scores: state.ledger.scores,
                pot_cents: state.ledger.pot_cents,
            },
            GameState::Playing(state) => Observation {
                phase: PhaseTag::Playing,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: Turn::Player(state.actor),
                round_index: state.ledger.round_index,
                hand_size: state.hand_size,
                private_hand: state.hands[viewer.index()].clone(),
                hand_counts: hand_counts(&state.hands),
                trump: Some(state.trump),
                current_trick: state.trick.clone(),
                bids: state.bids.map(Some),
                tricks_won: state.tricks_won,
                scores: state.ledger.scores,
                pot_cents: state.ledger.pot_cents,
            },
            GameState::Scoring(state) => Observation {
                phase: PhaseTag::Scoring,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: Turn::Environment,
                round_index: state.ledger.round_index,
                hand_size: state.hand_size,
                private_hand: Hand::default(),
                hand_counts: [0; PLAYERS],
                trump: Some(state.trump),
                current_trick: Trick::default(),
                bids: state.bids.map(Some),
                tricks_won: state.tricks_won,
                scores: state.ledger.scores,
                pot_cents: state.ledger.pot_cents,
            },
            GameState::Finished(state) => Observation {
                phase: PhaseTag::Finished,
                viewer,
                dealer: None,
                actor: Turn::Finished,
                round_index: round_count(PLAYERS),
                hand_size: 0,
                private_hand: Hand::default(),
                hand_counts: [0; PLAYERS],
                trump: None,
                current_trick: Trick::default(),
                bids: [None; PLAYERS],
                tricks_won: [0; PLAYERS],
                scores: state.scores,
                pot_cents: state.pot_cents,
            },
        }
    }
}

#[must_use]
pub const fn max_hand_size(players: usize) -> usize {
    let deck_limited = 51 / players;
    if deck_limited < MAX_HAND_SIZE {
        deck_limited
    } else {
        MAX_HAND_SIZE
    }
}

#[must_use]
pub const fn round_count(players: usize) -> usize {
    max_hand_size(players) * 2 - 1
}

#[must_use]
pub const fn hand_size_for_round(players: usize, round_index: usize) -> usize {
    let maximum = max_hand_size(players);
    if round_index < maximum {
        round_index + 1
    } else {
        maximum * 2 - round_index - 1
    }
}

#[must_use]
pub const fn score_round(bid: u8, tricks: u8, hand_size: u8) -> RoundScore {
    if tricks != bid {
        RoundScore {
            bid,
            tricks,
            outcome: BidOutcome::Missed,
            points: 0,
            payment_cents: MISSED_BID_PAYMENT_CENTS,
        }
    } else if tricks == hand_size {
        RoundScore {
            bid,
            tricks,
            outcome: BidOutcome::AllTricks,
            points: 20 + bid as u16,
            payment_cents: 0,
        }
    } else {
        RoundScore {
            bid,
            tricks,
            outcome: BidOutcome::Exact,
            points: 10 + bid as u16,
            payment_cents: 0,
        }
    }
}

/// Resolves the physical First Jack selection method for a supplied deck order.
///
/// This intentionally preserves the method's order bias: cards are dealt until
/// the first Jack rather than giving every seat an equal-sized draw.
#[must_use]
pub fn first_jack_recipient<const PLAYERS: usize>(
    first_recipient: Seat<PLAYERS>,
    deck: &DeckOrder,
) -> Option<Seat<PLAYERS>> {
    deck.cards()
        .iter()
        .position(|card| card.rank == Rank::Jack)
        .map(|index| first_recipient.advance(index % PLAYERS))
}

/// Performs one draw of the order-neutral High Card selection method.
///
/// Pass the returned `tied` mask to another draw with a freshly shuffled deck
/// when `selected` is `None`.
///
/// # Errors
///
/// Returns [`RuleViolation::NoSelectionCandidates`] when the mask is empty.
pub fn high_card_draw<const PLAYERS: usize>(
    candidates: [bool; PLAYERS],
    first_recipient: Seat<PLAYERS>,
    deck: &DeckOrder,
) -> Result<HighCardDraw<PLAYERS>, RuleViolation> {
    if !candidates.iter().any(|candidate| *candidate) {
        return Err(RuleViolation::NoSelectionCandidates);
    }
    let mut dealt = [None; PLAYERS];
    let mut cursor = 0;
    for offset in 0..PLAYERS {
        let seat = first_recipient.advance(offset);
        if candidates[seat.index()] {
            dealt[seat.index()] = Some(deck.cards()[cursor]);
            cursor += 1;
        }
    }
    let highest = dealt.iter().flatten().map(|card| card.rank).max();
    let tied = array::from_fn(|index| dealt[index].is_some_and(|card| Some(card.rank) == highest));
    let mut tied_seats = tied
        .iter()
        .enumerate()
        .filter_map(|(index, is_tied)| is_tied.then_some(Seat(index)));
    let first = tied_seats.next();
    let selected = if tied_seats.next().is_none() {
        first
    } else {
        None
    };
    Ok(HighCardDraw {
        dealt,
        tied,
        selected,
    })
}

fn validate_player_count(players: usize) -> Result<(), RuleViolation> {
    if (2..=51).contains(&players) {
        Ok(())
    } else {
        Err(RuleViolation::InvalidPlayerCount(players))
    }
}

fn deal<const PLAYERS: usize>(
    state: &AwaitingDeal<PLAYERS>,
    deck: &DeckOrder,
) -> Result<Transition<PLAYERS>, RuleViolation> {
    let hand_size = hand_size_for_round(PLAYERS, state.ledger.round_index);
    let mut hands: [Hand; PLAYERS] = array::from_fn(|_| Hand::default());
    let first = state.ledger.dealer.left();
    let mut cursor = 0;
    for _ in 0..hand_size {
        for offset in 0..PLAYERS {
            hands[first.advance(offset).index()].push(deck.0[cursor])?;
            cursor += 1;
        }
    }
    let trump = deck.0[cursor];
    cursor += 1;
    let mut stock = CardPile::default();
    for card in &deck.0[cursor..] {
        stock.push(*card)?;
    }
    let hand_size = u8::try_from(hand_size).expect("maximum Poche hand fits u8");
    let next = Game {
        state: GameState::Bidding(Bidding {
            ledger: state.ledger,
            hand_size,
            hands,
            trump,
            stock,
            bids: [None; PLAYERS],
            actor: first,
            bids_made: 0,
        }),
    };
    next.validate()?;
    Ok(Transition {
        next,
        round_scores: None,
    })
}

fn place_bid<const PLAYERS: usize>(
    state: &Bidding<PLAYERS>,
    player: Seat<PLAYERS>,
    tricks: u8,
) -> Result<Transition<PLAYERS>, RuleViolation> {
    if player != state.actor {
        return Err(RuleViolation::WrongActor);
    }
    if tricks > state.hand_size {
        return Err(RuleViolation::BidOutOfRange {
            bid: tricks,
            hand_size: state.hand_size,
        });
    }
    let mut next = state.clone();
    next.bids[player.index()] = Some(tricks);
    next.bids_made += 1;
    if next.bids_made == PLAYERS {
        let bids = next.bids.map(|bid| bid.expect("every player has bid"));
        let leader = state.ledger.dealer.left();
        Ok(Transition {
            next: Game {
                state: GameState::Playing(Playing {
                    ledger: state.ledger,
                    hand_size: state.hand_size,
                    hands: next.hands,
                    trump: state.trump,
                    stock: next.stock,
                    bids,
                    actor: leader,
                    leader,
                    trick: Trick::default(),
                    captured: array::from_fn(|_| CardPile::default()),
                    tricks_won: [0; PLAYERS],
                }),
            },
            round_scores: None,
        })
    } else {
        next.actor = state.actor.left();
        Ok(Transition {
            next: Game {
                state: GameState::Bidding(next),
            },
            round_scores: None,
        })
    }
}

fn legal_card_actions<const PLAYERS: usize>(state: &Playing<PLAYERS>) -> Vec<Action<PLAYERS>> {
    let hand = &state.hands[state.actor.index()];
    let lead_suit = state.trick.lead_suit();
    let must_follow = lead_suit.is_some_and(|suit| hand.contains_suit(suit));
    hand.iter()
        .filter(|card| !must_follow || Some(card.suit) == lead_suit)
        .map(|card| Action::Play {
            player: state.actor,
            card,
        })
        .collect()
}

fn play_card<const PLAYERS: usize>(
    state: &Playing<PLAYERS>,
    player: Seat<PLAYERS>,
    card: Card,
) -> Result<Transition<PLAYERS>, RuleViolation> {
    if player != state.actor {
        return Err(RuleViolation::WrongActor);
    }
    let hand = &state.hands[player.index()];
    if !hand.contains(card) {
        return Err(RuleViolation::CardNotInHand(card));
    }
    if let Some(lead_suit) = state.trick.lead_suit()
        && hand.contains_suit(lead_suit)
        && card.suit != lead_suit
    {
        return Err(RuleViolation::MustFollowSuit(lead_suit));
    }

    let mut next = state.clone();
    next.hands[player.index()].remove(card)?;
    next.trick.push(PlayedCard { player, card })?;
    if next.trick.len() == PLAYERS {
        complete_trick(next)
    } else {
        next.actor = player.left();
        checked_playing_transition(next)
    }
}

fn complete_trick<const PLAYERS: usize>(
    mut state: Playing<PLAYERS>,
) -> Result<Transition<PLAYERS>, RuleViolation> {
    let winner = state
        .trick
        .winner(state.trump.suit)
        .expect("a complete trick has a winner");
    for played in state.trick.iter() {
        state.captured[winner.index()].push(played.card)?;
    }
    state.tricks_won[winner.index()] += 1;
    state.trick = Trick::default();
    state.actor = winner;
    state.leader = winner;

    if state.hands.iter().all(FixedCards::is_empty) {
        let next = Game {
            state: GameState::Scoring(Scoring {
                ledger: state.ledger,
                hand_size: state.hand_size,
                trump: state.trump,
                stock: state.stock,
                bids: state.bids,
                captured: state.captured,
                tricks_won: state.tricks_won,
            }),
        };
        next.validate()?;
        Ok(Transition {
            next,
            round_scores: None,
        })
    } else {
        checked_playing_transition(state)
    }
}

fn checked_playing_transition<const PLAYERS: usize>(
    state: Playing<PLAYERS>,
) -> Result<Transition<PLAYERS>, RuleViolation> {
    let next = Game {
        state: GameState::Playing(state),
    };
    next.validate()?;
    Ok(Transition {
        next,
        round_scores: None,
    })
}

fn settle_round<const PLAYERS: usize>(state: &Scoring<PLAYERS>) -> Transition<PLAYERS> {
    let round_scores: [RoundScore; PLAYERS] = array::from_fn(|index| {
        score_round(state.bids[index], state.tricks_won[index], state.hand_size)
    });
    let mut ledger = state.ledger;
    for (index, score) in round_scores.iter().enumerate() {
        ledger.scores[index] += score.points;
        ledger.pot_cents += score.payment_cents;
    }

    ledger.round_index += 1;
    let next_state = if ledger.round_index == round_count(PLAYERS) {
        GameState::Finished(Finished {
            scores: ledger.scores,
            pot_cents: ledger.pot_cents,
            winners: winner_mask(ledger.scores),
        })
    } else {
        ledger.dealer = ledger.dealer.left();
        GameState::AwaitingDeal(AwaitingDeal { ledger })
    };
    Transition {
        next: Game { state: next_state },
        round_scores: Some(round_scores),
    }
}

fn winner_mask<const PLAYERS: usize>(scores: [u16; PLAYERS]) -> [bool; PLAYERS] {
    let maximum = *scores.iter().max().expect("Poche has players");
    scores.map(|score| score == maximum)
}

fn validate_card_zones<const PLAYERS: usize>(
    hands: &[Hand; PLAYERS],
    trump: Card,
    stock: &CardPile,
    trick: &Trick<PLAYERS>,
    captured: &[CardPile; PLAYERS],
) -> Result<(), RuleViolation> {
    let mut counts = [0_u8; DECK_SIZE];
    counts[trump.index()] += 1;
    for card in hands
        .iter()
        .flat_map(FixedCards::iter)
        .chain(stock.iter())
        .chain(trick.iter().map(|played| played.card))
        .chain(captured.iter().flat_map(FixedCards::iter))
    {
        counts[card.index()] += 1;
    }
    for (index, count) in counts.into_iter().enumerate() {
        if count != 1 {
            return Err(RuleViolation::CardConservation {
                card: Card::standard_deck()[index],
                count,
            });
        }
    }
    Ok(())
}

fn hand_counts<const PLAYERS: usize>(hands: &[Hand; PLAYERS]) -> [u8; PLAYERS] {
    array::from_fn(|index| u8::try_from(hands[index].len()).expect("Poche hand fits u8"))
}

fn empty_observation<const PLAYERS: usize>(
    viewer: Seat<PLAYERS>,
    phase: PhaseTag,
    actor: Turn<PLAYERS>,
    ledger: Option<Ledger<PLAYERS>>,
) -> Observation<PLAYERS> {
    Observation {
        phase,
        viewer,
        dealer: ledger.map(|value| value.dealer),
        actor,
        round_index: ledger.map_or(0, |value| value.round_index),
        hand_size: ledger.map_or(0, |value| {
            u8::try_from(hand_size_for_round(PLAYERS, value.round_index))
                .expect("Poche hand fits u8")
        }),
        private_hand: Hand::default(),
        hand_counts: [0; PLAYERS],
        trump: None,
        current_trick: Trick::default(),
        bids: [None; PLAYERS],
        tricks_won: [0; PLAYERS],
        scores: ledger.map_or([0; PLAYERS], |value| value.scores),
        pot_cents: ledger.map_or(0, |value| value.pot_cents),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat<const PLAYERS: usize>(index: usize) -> Seat<PLAYERS> {
        Seat::new(index).expect("valid test seat")
    }

    fn deck_starting(cards: &[Card]) -> DeckOrder {
        let mut ordered = Card::standard_deck();
        let mut cursor = 0;
        for card in cards {
            ordered[cursor] = *card;
            cursor += 1;
        }
        for candidate in Card::standard_deck() {
            if !cards.contains(&candidate) {
                ordered[cursor] = candidate;
                cursor += 1;
            }
        }
        DeckOrder::new(ordered).expect("unique test deck")
    }

    fn act<const PLAYERS: usize>(game: &Game<PLAYERS>, action: Action<PLAYERS>) -> Game<PLAYERS> {
        game.transition(action).expect("valid action").next
    }

    fn play_scripted_game<const PLAYERS: usize>() -> Game<PLAYERS> {
        let mut game = Game::new(seat(0)).expect("valid game");
        for _ in 0..2_000 {
            game.validate().expect("cards stay conserved");
            let action = match game.turn() {
                Turn::Chance => Action::Deal(DeckOrder::standard()),
                Turn::Player(_) => game
                    .legal_player_actions()
                    .into_iter()
                    .next()
                    .expect("acting player has a legal action"),
                Turn::Environment => Action::SettleRound,
                Turn::Finished => return game,
            };
            game = act(&game, action);
        }
        panic!("scripted game exceeded step bound");
    }

    #[test]
    fn parametric_hand_schedule_covers_every_supported_player_count() {
        for players in 2..=51 {
            let maximum = max_hand_size(players);
            assert!((1..=7).contains(&maximum));
            assert!(players * maximum < DECK_SIZE);
            assert_eq!(round_count(players), maximum * 2 - 1);
            assert_eq!(hand_size_for_round(players, 0), 1);
            assert_eq!(hand_size_for_round(players, round_count(players) - 1), 1);
            assert_eq!(hand_size_for_round(players, maximum - 1), maximum);
        }
        assert_eq!(max_hand_size(2), 7);
        assert_eq!(max_hand_size(51), 1);
        assert!(matches!(
            Seat::<1>::new(0),
            Err(RuleViolation::InvalidPlayerCount(1))
        ));
        assert!(matches!(
            Seat::<52>::new(0),
            Err(RuleViolation::InvalidPlayerCount(52))
        ));
    }

    #[test]
    fn deck_validation_rejects_duplicates() {
        let mut cards = Card::standard_deck();
        cards[1] = cards[0];
        assert!(matches!(
            DeckOrder::new(cards),
            Err(RuleViolation::DuplicateCard(_))
        ));
    }

    #[test]
    fn physical_player_selection_preserves_each_method() {
        let first_jack_deck = deck_starting(&[
            Card::new(Suit::Clubs, Rank::Two),
            Card::new(Suit::Diamonds, Rank::Jack),
        ]);
        assert_eq!(
            first_jack_recipient(seat::<3>(2), &first_jack_deck),
            Some(seat(0))
        );

        let tied_draw = deck_starting(&[
            Card::new(Suit::Clubs, Rank::Ace),
            Card::new(Suit::Diamonds, Rank::Ace),
            Card::new(Suit::Hearts, Rank::King),
        ]);
        let result = high_card_draw([true; 3], seat(0), &tied_draw).expect("draw succeeds");
        assert_eq!(result.selected, None);
        assert_eq!(result.tied, [true, true, false]);

        let tie_break = deck_starting(&[
            Card::new(Suit::Spades, Rank::Ace),
            Card::new(Suit::Clubs, Rank::King),
        ]);
        let result = high_card_draw(result.tied, seat(0), &tie_break).expect("draw succeeds");
        assert_eq!(result.selected, Some(seat(0)));
    }

    #[test]
    fn deal_bid_play_and_score_one_card_round() {
        let game = Game::<2>::new(seat(0)).expect("valid game");
        assert_eq!(game.turn(), Turn::Chance);
        assert_eq!(game.observe(seat(1)).pot_cents, 50);

        let game = act(&game, Action::Deal(DeckOrder::standard()));
        let GameState::Bidding(bidding) = game.state() else {
            panic!("expected bidding");
        };
        assert_eq!(bidding.actor, seat(1));
        assert_eq!(bidding.hands[0].len(), 1);
        assert_eq!(bidding.hands[1].len(), 1);
        assert_eq!(bidding.stock.len(), 49);
        game.validate().expect("cards conserved after deal");

        let game = act(
            &game,
            Action::Bid {
                player: seat(1),
                tricks: 0,
            },
        );
        let game = act(
            &game,
            Action::Bid {
                player: seat(0),
                tricks: 1,
            },
        );
        let first = match game.state() {
            GameState::Playing(state) => state.hands[1].iter().next().expect("card"),
            _ => panic!("expected play"),
        };
        let game = act(
            &game,
            Action::Play {
                player: seat(1),
                card: first,
            },
        );
        let second = match game.state() {
            GameState::Playing(state) => state.hands[0].iter().next().expect("card"),
            _ => panic!("expected play"),
        };
        let game = act(
            &game,
            Action::Play {
                player: seat(0),
                card: second,
            },
        );
        assert_eq!(game.phase(), PhaseTag::Scoring);
        game.validate().expect("cards conserved before scoring");
        let transition = game
            .transition(Action::SettleRound)
            .expect("settle succeeds");
        assert!(transition.round_scores.is_some());
        assert_eq!(transition.next.phase(), PhaseTag::AwaitingDeal);
        let observation = transition.next.observe(seat(0));
        assert_eq!(observation.round_index, 1);
        assert_eq!(observation.dealer, Some(seat(1)));
    }

    #[test]
    fn bid_domain_and_total_are_exactly_the_rulebook_domain() {
        let game = Game::<2>::new(seat(0)).expect("valid game");
        let game = act(&game, Action::Deal(DeckOrder::standard()));
        assert!(matches!(
            game.transition(Action::Bid {
                player: seat(0),
                tricks: 0,
            }),
            Err(RuleViolation::WrongActor)
        ));
        assert!(matches!(
            game.transition(Action::Bid {
                player: seat(1),
                tricks: 2,
            }),
            Err(RuleViolation::BidOutOfRange {
                bid: 2,
                hand_size: 1,
            })
        ));
        let game = act(
            &game,
            Action::Bid {
                player: seat(1),
                tricks: 1,
            },
        );
        let game = act(
            &game,
            Action::Bid {
                player: seat(0),
                tricks: 1,
            },
        );
        assert_eq!(game.phase(), PhaseTag::Playing);
    }

    #[test]
    fn legal_actions_enforce_follow_suit() {
        let club_two = Card::new(Suit::Clubs, Rank::Two);
        let club_three = Card::new(Suit::Clubs, Rank::Three);
        let spade_two = Card::new(Suit::Spades, Rank::Two);
        let spade_three = Card::new(Suit::Spades, Rank::Three);
        let deck = deck_starting(&[
            club_two,
            club_three,
            spade_two,
            spade_three,
            Card::new(Suit::Hearts, Rank::Two),
        ]);
        let ledger = Ledger {
            dealer: seat::<2>(1),
            round_index: 1,
            scores: [0; 2],
            pot_cents: 50,
        };
        let game = Game {
            state: GameState::AwaitingDeal(AwaitingDeal { ledger }),
        };
        let game = act(&game, Action::Deal(deck));
        let game = act(
            &game,
            Action::Bid {
                player: seat(0),
                tricks: 0,
            },
        );
        let game = act(
            &game,
            Action::Bid {
                player: seat(1),
                tricks: 0,
            },
        );
        let game = act(
            &game,
            Action::Play {
                player: seat(0),
                card: club_two,
            },
        );
        assert_eq!(
            game.legal_player_actions(),
            vec![Action::Play {
                player: seat(1),
                card: club_three,
            }]
        );
        assert!(matches!(
            game.transition(Action::Play {
                player: seat(1),
                card: spade_three,
            }),
            Err(RuleViolation::MustFollowSuit(Suit::Clubs))
        ));
    }

    #[test]
    fn trick_winner_uses_trump_then_lead_then_rank() {
        let mut trick = Trick::<3>::default();
        trick
            .push(PlayedCard {
                player: seat(0),
                card: Card::new(Suit::Hearts, Rank::Ace),
            })
            .unwrap();
        trick
            .push(PlayedCard {
                player: seat(1),
                card: Card::new(Suit::Spades, Rank::Ace),
            })
            .unwrap();
        trick
            .push(PlayedCard {
                player: seat(2),
                card: Card::new(Suit::Clubs, Rank::Two),
            })
            .unwrap();

        assert_eq!(trick.winner(Suit::Clubs), Some(seat(2)));
        assert_eq!(trick.winner(Suit::Diamonds), Some(seat(0)));
    }

    #[test]
    fn score_outcomes_keep_money_separate() {
        assert_eq!(
            score_round(2, 1, 3),
            RoundScore {
                bid: 2,
                tricks: 1,
                outcome: BidOutcome::Missed,
                points: 0,
                payment_cents: 10,
            }
        );
        assert_eq!(score_round(0, 0, 3).points, 10);
        assert_eq!(score_round(3, 3, 3).points, 23);
        assert_eq!(score_round(3, 3, 3).payment_cents, 0);
        assert_eq!(
            score_round(0, 0, 3).score_cell(),
            ScoreCell::Exact {
                bid: 0,
                written: 10,
            }
        );
        assert_eq!(
            score_round(3, 3, 3).score_cell(),
            ScoreCell::AllTricks {
                bid: 3,
                written: 23,
            }
        );

        let finished = Finished::<3> {
            scores: [40, 40, 10],
            pot_cents: 101,
            winners: [true, true, false],
        };
        assert_eq!(
            finished.pot_division(),
            Some(PotDivision {
                winner_count: 2,
                cents_per_winner: 50,
                remainder_cents: 1,
            })
        );
        assert_eq!(winner_mask([40, 40, 10]), [true, true, false]);
    }

    #[test]
    fn replay_is_deterministic_and_observation_hides_other_hands() {
        let start = Game::<2>::new(seat(0)).expect("valid game");
        let left = act(&start, Action::Deal(DeckOrder::standard()));
        let right = act(&start, Action::Deal(DeckOrder::standard()));
        assert_eq!(left, right);

        let observation = left.observe(seat(0));
        let GameState::Bidding(state) = left.state() else {
            panic!("expected bidding");
        };
        assert_eq!(observation.private_hand, state.hands[0]);
        assert_eq!(observation.hand_counts, [1, 1]);
        assert!(
            !observation
                .private_hand
                .contains(state.hands[1].iter().next().unwrap())
        );
    }

    #[test]
    fn scripted_games_terminate_for_smallest_and_largest_tables() {
        let two_player = play_scripted_game::<2>();
        let two_player_replay = play_scripted_game::<2>();
        assert_eq!(two_player, two_player_replay);
        assert!(matches!(two_player.state(), GameState::Finished(_)));

        let fifty_one_player = play_scripted_game::<51>();
        assert!(matches!(fifty_one_player.state(), GameState::Finished(_)));
    }
}

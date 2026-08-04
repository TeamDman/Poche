use std::error::Error;
use std::fmt;

use facet::Facet;
use poche_domain::{FiniteDomain, MicroPartition, MicroZone};

/// Failure to construct or evolve a refined micro-model value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// A bounded value was outside its semantic domain.
    OutOfRange {
        /// Domain name.
        domain: &'static str,
        /// Rejected integer.
        value: u16,
    },
    /// A card occurs in more than one semantic zone.
    DuplicateCard(Card),
    /// Card zones do not contain the complete six-card deck.
    IncompletePartition,
    /// A requested card was absent from a card set.
    CardAbsent(Card),
    /// A bid does not fit the scheduled hand size.
    BidExceedsHand {
        /// Rejected bid.
        bid: Bid,
        /// Current hand size.
        hand_size: u8,
    },
    /// A chance deal variant does not match the scheduled round hand size.
    DealHandMismatch {
        /// Scheduled hand size.
        expected: u8,
        /// Supplied deal hand size.
        actual: u8,
    },
    /// An action was supplied to a phase that does not admit it.
    WrongPhase,
    /// A player action named a seat other than the acting player.
    WrongActor {
        /// Required actor.
        expected: Player,
        /// Supplied actor.
        actual: Player,
    },
    /// A card violates the mandatory follow-suit rule.
    MustFollowSuit {
        /// Required lead suit identity.
        suit: u8,
    },
    /// A lowered pure formal computation failed to build or evaluate.
    Formal(Box<poche_formal::Diagnostic>),
    /// An internal transition would violate a named semantic invariant.
    Invariant(&'static str),
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange { domain, value } => {
                write!(formatter, "{value} is outside finite domain {domain}")
            }
            Self::DuplicateCard(card) => write!(formatter, "card {card:?} occurs more than once"),
            Self::IncompletePartition => {
                formatter.write_str("card zones do not partition the deck")
            }
            Self::CardAbsent(card) => write!(formatter, "card {card:?} is absent"),
            Self::BidExceedsHand { bid, hand_size } => {
                write!(formatter, "bid {} exceeds hand size {hand_size}", bid.get())
            }
            Self::DealHandMismatch { expected, actual } => {
                write!(
                    formatter,
                    "scheduled hand size {expected} rejects {actual}-card deal"
                )
            }
            Self::WrongPhase => formatter.write_str("action is unavailable in this phase"),
            Self::WrongActor { expected, actual } => {
                write!(
                    formatter,
                    "expected actor {expected:?}, received {actual:?}"
                )
            }
            Self::MustFollowSuit { suit } => {
                write!(formatter, "must follow lead suit {suit}")
            }
            Self::Formal(error) => write!(formatter, "formal computation failed: {error}"),
            Self::Invariant(invariant) => write!(formatter, "state violates {invariant}"),
        }
    }
}

impl Error for ModelError {}

impl From<poche_formal::Diagnostic> for ModelError {
    fn from(value: poche_formal::Diagnostic) -> Self {
        Self::Formal(Box::new(value))
    }
}

/// One of the two fixed seats in the exhaustive micro-scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum Player {
    /// First seat.
    Zero,
    /// Second seat.
    One,
}

impl Player {
    /// Both seats in canonical order.
    pub const ALL: [Self; 2] = [Self::Zero, Self::One];

    /// Return the other/left seat in the two-player clockwise ring.
    #[must_use]
    pub const fn left(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One => Self::Zero,
        }
    }

    /// Return the fixed-array index.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl FiniteDomain for Player {
    fn cardinality() -> u128 {
        2
    }

    fn encode(&self) -> u128 {
        *self as u128
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        match code {
            0 => Ok(Self::Zero),
            1 => Ok(Self::One),
            _ => Err(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "Player",
            }),
        }
    }
}

/// One of six cards: two unranked suits and three ascending ranks per suit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum Card {
    /// Suit zero, lowest rank.
    S0R0,
    /// Suit zero, middle rank.
    S0R1,
    /// Suit zero, highest rank.
    S0R2,
    /// Suit one, lowest rank.
    S1R0,
    /// Suit one, middle rank.
    S1R1,
    /// Suit one, highest rank.
    S1R2,
}

impl Card {
    /// All cards in dense canonical order.
    pub const ALL: [Self; 6] = [
        Self::S0R0,
        Self::S0R1,
        Self::S0R2,
        Self::S1R0,
        Self::S1R1,
        Self::S1R2,
    ];

    /// Dense identity in `0..6`.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Unordered suit identity in `0..2`.
    #[must_use]
    pub const fn suit(self) -> u8 {
        (self as u8) / 3
    }

    /// Ordered rank identity in `0..3`.
    #[must_use]
    pub const fn rank(self) -> u8 {
        (self as u8) % 3
    }
}

impl FiniteDomain for Card {
    fn cardinality() -> u128 {
        6
    }

    fn encode(&self) -> u128 {
        *self as u128
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        Self::ALL
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .copied()
            .ok_or(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "Card",
            })
    }
}

/// A fixed six-bit set of card identities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct CardSet(u8);

impl CardSet {
    const ALL_BITS: u8 = 0b11_1111;

    /// The empty set.
    pub const EMPTY: Self = Self(0);

    /// Construct a set while rejecting repeated input cards.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::DuplicateCard`] for a repeated input.
    pub fn from_cards(cards: impl IntoIterator<Item = Card>) -> Result<Self, ModelError> {
        let mut result = Self::EMPTY;
        for card in cards {
            if result.contains(card) {
                return Err(ModelError::DuplicateCard(card));
            }
            result.insert(card);
        }
        Ok(result)
    }

    /// Return whether the set contains `card`.
    #[must_use]
    pub const fn contains(self, card: Card) -> bool {
        self.0 & (1 << card as u8) != 0
    }

    /// Return whether any contained card has `suit`.
    #[must_use]
    pub fn contains_suit(self, suit: u8) -> bool {
        self.iter().any(|card| card.suit() == suit)
    }

    /// Number of contained cards.
    #[must_use]
    pub fn len(self) -> u8 {
        u8::try_from(self.0.count_ones()).unwrap_or(u8::MAX)
    }

    /// Return whether no cards are contained.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Enumerate contained cards in dense identity order.
    pub fn iter(self) -> impl Iterator<Item = Card> {
        Card::ALL
            .into_iter()
            .filter(move |card| self.contains(*card))
    }

    /// Remove a present card.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::CardAbsent`] when the card is not present.
    pub fn without(mut self, card: Card) -> Result<Self, ModelError> {
        if !self.contains(card) {
            return Err(ModelError::CardAbsent(card));
        }
        self.0 &= !(1 << card as u8);
        Ok(self)
    }

    pub(crate) fn with(mut self, card: Card) -> Result<Self, ModelError> {
        if self.contains(card) {
            return Err(ModelError::DuplicateCard(card));
        }
        self.insert(card);
        Ok(self)
    }

    pub(crate) fn union(self, other: Self) -> Result<Self, ModelError> {
        if self.bits() & other.bits() != 0 {
            let duplicate = other
                .iter()
                .find(|card| self.contains(*card))
                .unwrap_or(Card::S0R0);
            return Err(ModelError::DuplicateCard(duplicate));
        }
        Ok(Self(self.bits() | other.bits()))
    }

    pub(crate) const fn bits(self) -> u8 {
        self.0
    }

    pub(crate) fn insert(&mut self, card: Card) {
        self.0 |= 1 << card as u8;
    }
}

impl FiniteDomain for CardSet {
    fn cardinality() -> u128 {
        64
    }

    fn encode(&self) -> u128 {
        u128::from(self.0)
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        if code < Self::cardinality() {
            Ok(Self(u8::try_from(code).expect("code below 64 fits u8")))
        } else {
            Err(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "CardSet",
            })
        }
    }
}

/// One exact maximum-round partition: two cards per player, one trump, and one
/// undealt card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub struct TwoCardDeal {
    hands: [CardSet; 2],
    trump: Card,
    undealt: CardSet,
}

impl TwoCardDeal {
    /// Validate exact zone counts, disjointness, and conservation.
    ///
    /// # Errors
    ///
    /// Returns an error unless hands, trump, and undealt partition all six
    /// cards with counts `[2, 2, 1, 1]`.
    pub fn new(hands: [CardSet; 2], trump: Card, undealt: CardSet) -> Result<Self, ModelError> {
        if hands[0].len() != 2 || hands[1].len() != 2 || undealt.len() != 1 {
            return Err(ModelError::IncompletePartition);
        }
        let mut seen = 1_u8 << trump as u8;
        for zone in [hands[0], hands[1], undealt] {
            if seen & zone.bits() != 0 {
                let card = zone
                    .iter()
                    .find(|card| seen & (1 << *card as u8) != 0)
                    .unwrap_or(Card::S0R0);
                return Err(ModelError::DuplicateCard(card));
            }
            seen |= zone.bits();
        }
        if seen != CardSet::ALL_BITS {
            return Err(ModelError::IncompletePartition);
        }
        Ok(Self {
            hands,
            trump,
            undealt,
        })
    }

    /// The maximum two-card hand zone for `player`.
    #[must_use]
    pub const fn hand(self, player: Player) -> CardSet {
        self.hands[player.index()]
    }

    /// Revealed trump card.
    #[must_use]
    pub const fn trump(self) -> Card {
        self.trump
    }

    /// Single card outside maximum-size hands and trump.
    #[must_use]
    pub const fn undealt(self) -> CardSet {
        self.undealt
    }

    /// Enumerate all 180 semantically distinct partitions.
    #[must_use]
    pub fn all() -> Vec<Self> {
        MicroPartition::enumerate()
            .into_iter()
            .map(|partition| Self::from_partition(&partition))
            .collect()
    }

    fn from_partition(partition: &MicroPartition) -> Self {
        let mut hands = [CardSet::EMPTY; 2];
        let mut trump = None;
        let mut undealt = CardSet::EMPTY;
        for (index, zone) in partition.zones().iter().enumerate() {
            let card = Card::ALL[index];
            match zone {
                MicroZone::Player0 => hands[0].insert(card),
                MicroZone::Player1 => hands[1].insert(card),
                MicroZone::Trump => trump = Some(card),
                MicroZone::Undealt => undealt.insert(card),
            }
        }
        Self {
            hands,
            trump: trump.expect("micro partition has exactly one trump"),
            undealt,
        }
    }

    fn partition(self) -> MicroPartition {
        let zones = Card::ALL.map(|card| {
            if self.hands[0].contains(card) {
                MicroZone::Player0
            } else if self.hands[1].contains(card) {
                MicroZone::Player1
            } else if self.trump == card {
                MicroZone::Trump
            } else {
                MicroZone::Undealt
            }
        });
        MicroPartition::new(zones).expect("validated deal has a canonical partition")
    }
}

impl FiniteDomain for TwoCardDeal {
    fn cardinality() -> u128 {
        MicroPartition::cardinality()
    }

    fn encode(&self) -> u128 {
        self.partition().encode()
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        MicroPartition::decode(code).map(|partition| Self::from_partition(&partition))
    }
}

/// One exact one-card-round partition: one card per player, one trump, and
/// three undealt cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub struct OneCardDeal {
    hands: [Card; 2],
    trump: Card,
    undealt: CardSet,
}

impl OneCardDeal {
    /// Validate exact zone counts, disjointness, and conservation.
    ///
    /// # Errors
    ///
    /// Returns an error unless the zones partition all six cards with counts
    /// `[1, 1, 1, 3]`.
    pub fn new(hands: [Card; 2], trump: Card, undealt: CardSet) -> Result<Self, ModelError> {
        if undealt.len() != 3 {
            return Err(ModelError::IncompletePartition);
        }
        let hand_sets = [
            CardSet::from_cards([hands[0]])?,
            CardSet::from_cards([hands[1]])?,
        ];
        let mut seen = 1_u8 << trump as u8;
        for zone in [hand_sets[0], hand_sets[1], undealt] {
            if seen & zone.bits() != 0 {
                let card = zone
                    .iter()
                    .find(|card| seen & (1 << *card as u8) != 0)
                    .unwrap_or(Card::S0R0);
                return Err(ModelError::DuplicateCard(card));
            }
            seen |= zone.bits();
        }
        if seen != CardSet::ALL_BITS {
            return Err(ModelError::IncompletePartition);
        }
        Ok(Self {
            hands,
            trump,
            undealt,
        })
    }

    /// Singleton hand for `player`.
    #[must_use]
    pub fn hand(self, player: Player) -> CardSet {
        CardSet(1 << self.hands[player.index()] as u8)
    }

    /// Revealed trump card.
    #[must_use]
    pub const fn trump(self) -> Card {
        self.trump
    }

    /// Three cards outside hands and trump.
    #[must_use]
    pub const fn undealt(self) -> CardSet {
        self.undealt
    }

    /// Enumerate all 120 semantically distinct one-card partitions.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut deals = Vec::with_capacity(120);
        for first in Card::ALL {
            for second in Card::ALL {
                if second == first {
                    continue;
                }
                for trump in Card::ALL {
                    if trump == first || trump == second {
                        continue;
                    }
                    let mut undealt = CardSet::EMPTY;
                    for card in Card::ALL {
                        if ![first, second, trump].contains(&card) {
                            undealt.insert(card);
                        }
                    }
                    deals.push(Self {
                        hands: [first, second],
                        trump,
                        undealt,
                    });
                }
            }
        }
        deals
    }
}

impl FiniteDomain for OneCardDeal {
    fn cardinality() -> u128 {
        120
    }

    fn encode(&self) -> u128 {
        Self::all()
            .into_iter()
            .position(|candidate| candidate == *self)
            .expect("validated one-card deal is enumerated") as u128
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        if code >= Self::cardinality() {
            return Err(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "OneCardDeal",
            });
        }
        Ok(Self::all()[usize::try_from(code).expect("one-card deal code fits usize")])
    }
}

/// Phase-compatible chance deal for either scheduled hand size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum Deal {
    /// One-card round partition.
    One(OneCardDeal),
    /// Two-card round partition.
    Two(TwoCardDeal),
}

impl Deal {
    /// Enumerate every deal matching `round`.
    #[must_use]
    pub fn all_for(round: RoundId) -> Vec<Self> {
        if round.hand_size() == 1 {
            OneCardDeal::all().into_iter().map(Self::One).collect()
        } else {
            TwoCardDeal::all().into_iter().map(Self::Two).collect()
        }
    }

    /// Exact cards held by both players.
    #[must_use]
    pub fn hands(self) -> [CardSet; 2] {
        match self {
            Self::One(deal) => [deal.hand(Player::Zero), deal.hand(Player::One)],
            Self::Two(deal) => [deal.hand(Player::Zero), deal.hand(Player::One)],
        }
    }

    /// Revealed trump card.
    #[must_use]
    pub const fn trump(self) -> Card {
        match self {
            Self::One(deal) => deal.trump(),
            Self::Two(deal) => deal.trump(),
        }
    }

    /// Cards outside player hands and trump.
    #[must_use]
    pub const fn undealt(self) -> CardSet {
        match self {
            Self::One(deal) => deal.undealt(),
            Self::Two(deal) => deal.undealt(),
        }
    }

    /// Exact cards per player.
    #[must_use]
    pub const fn hand_size(self) -> u8 {
        match self {
            Self::One(_) => 1,
            Self::Two(_) => 2,
        }
    }

    /// Validate this deal against a scheduled round.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::DealHandMismatch`] for the wrong variant.
    pub fn validate_round(self, round: RoundId) -> Result<(), ModelError> {
        if self.hand_size() == round.hand_size() {
            Ok(())
        } else {
            Err(ModelError::DealHandMismatch {
                expected: round.hand_size(),
                actual: self.hand_size(),
            })
        }
    }
}

impl FiniteDomain for Deal {
    fn cardinality() -> u128 {
        OneCardDeal::cardinality() + TwoCardDeal::cardinality()
    }

    fn encode(&self) -> u128 {
        match self {
            Self::One(deal) => deal.encode(),
            Self::Two(deal) => OneCardDeal::cardinality() + deal.encode(),
        }
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        if code < OneCardDeal::cardinality() {
            OneCardDeal::decode(code).map(Self::One)
        } else {
            TwoCardDeal::decode(code - OneCardDeal::cardinality()).map(Self::Two)
        }
    }
}

/// Round identity for the complete micro-game schedule `1, 2, 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum RoundId {
    /// Ascending one-card round.
    OneAscending,
    /// Maximum two-card round.
    Two,
    /// Final descending one-card round.
    OneDescending,
}

impl RoundId {
    /// All three scheduled rounds in order.
    pub const ALL: [Self; 3] = [Self::OneAscending, Self::Two, Self::OneDescending];

    /// Scheduled hand size.
    #[must_use]
    pub const fn hand_size(self) -> u8 {
        match self {
            Self::OneAscending | Self::OneDescending => 1,
            Self::Two => 2,
        }
    }

    /// Following round, absent after the final descending one-card round.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::OneAscending => Some(Self::Two),
            Self::Two => Some(Self::OneDescending),
            Self::OneDescending => None,
        }
    }
}

impl FiniteDomain for RoundId {
    fn cardinality() -> u128 {
        3
    }

    fn encode(&self) -> u128 {
        *self as u128
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        Self::ALL
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .copied()
            .ok_or(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "RoundId",
            })
    }
}

/// Fixed whole-number bid domain for the maximum two-card scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum Bid {
    /// Zero tricks.
    Zero,
    /// One trick.
    One,
    /// Two tricks.
    Two,
}

impl Bid {
    /// Every maximum-scope bid in ascending order.
    pub const ALL: [Self; 3] = [Self::Zero, Self::One, Self::Two];

    /// Numeric bid.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// Return whether the bid fits this scheduled round.
    #[must_use]
    pub const fn legal_for(self, round: RoundId) -> bool {
        self.get() <= round.hand_size()
    }
}

impl FiniteDomain for Bid {
    fn cardinality() -> u128 {
        3
    }

    fn encode(&self) -> u128 {
        *self as u128
    }

    fn decode(code: u128) -> Result<Self, poche_domain::DecodeError> {
        Self::ALL
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .copied()
            .ok_or(poche_domain::DecodeError::OutOfRange {
                code,
                cardinality: Self::cardinality(),
                domain: "Bid",
            })
    }
}

/// Tricks won in a round, bounded by the maximum two-card hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct Tricks(u8);

impl Tricks {
    /// Construct a trick count in `0..=2`.
    ///
    /// # Errors
    ///
    /// Returns an error above two.
    pub fn new(value: u8) -> Result<Self, ModelError> {
        if value <= 2 {
            Ok(Self(value))
        } else {
            Err(ModelError::OutOfRange {
                domain: "Tricks",
                value: u16::from(value),
            })
        }
    }

    /// Numeric count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn increment(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Cumulative micro-game points, bounded by `21 + 22 + 21 = 64`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct Score(u8);

impl Score {
    /// Zero cumulative points.
    pub const ZERO: Self = Self(0);

    /// Numeric cumulative score.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn add(self, points: u8) -> Result<Self, ModelError> {
        let value = self.0.checked_add(points).ok_or(ModelError::OutOfRange {
            domain: "Score",
            value: u16::from(self.0) + u16::from(points),
        })?;
        if value <= 64 {
            Ok(Self(value))
        } else {
            Err(ModelError::OutOfRange {
                domain: "Score",
                value: u16::from(value),
            })
        }
    }
}

/// Communal pot represented by missed-payment count after the fixed 50-cent
/// two-player ante. It therefore ranges from 50 through 110 cents in steps of 10.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct Pot(u8);

impl Pot {
    /// Opening two-player ante.
    pub const OPENING: Self = Self(0);

    /// Pot value in cents.
    #[must_use]
    pub const fn cents(self) -> u16 {
        50 + (self.0 as u16) * 10
    }

    pub(crate) fn add_miss(self) -> Result<Self, ModelError> {
        if self.0 < 6 {
            Ok(Self(self.0 + 1))
        } else {
            Err(ModelError::OutOfRange {
                domain: "Pot",
                value: u16::from(self.0) + 1,
            })
        }
    }
}

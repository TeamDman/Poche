// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Canonical finite domains and semantic refinements for formal Poche models.
//!
//! Encoding is deliberately stricter than ordinary Rust conversion: every
//! domain has deterministic enumeration and a dense canonical code range, and
//! decoding rejects spare codes and invalid refinements.

use core::fmt;
use core::marker::PhantomData;

use facet::Facet;

/// A canonical finite type with a dense encoding in `0..cardinality()`.
pub trait FiniteDomain: Clone + Eq + Sized {
    /// Number of distinct semantic values.
    fn cardinality() -> u128;

    /// Encode a value into the canonical dense range.
    fn encode(&self) -> u128;

    /// Decode a canonical value, rejecting every spare bit pattern.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] when `code` is not a canonical value.
    fn decode(code: u128) -> Result<Self, DecodeError>;

    /// Deterministically enumerate values in increasing canonical-code order.
    #[must_use]
    fn enumerate() -> Vec<Self> {
        (0..Self::cardinality())
            .map(|code| Self::decode(code).expect("codes below cardinality are valid"))
            .collect()
    }

    /// Minimum number of bits required by the canonical code.
    #[must_use]
    fn bit_width() -> u32 {
        let maximum = Self::cardinality().saturating_sub(1);
        u128::BITS - maximum.leading_zeros()
    }
}

/// A failed finite-domain decode or semantic-refinement construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// A dense code was outside `0..cardinality`.
    OutOfRange {
        /// Rejected code.
        code: u128,
        /// Exclusive upper bound.
        cardinality: u128,
        /// Domain name used in diagnostics.
        domain: &'static str,
    },
    /// A shaped value did not satisfy the semantic refinement.
    InvalidRefinement {
        /// Refinement name.
        refinement: &'static str,
        /// Stable diagnostic reason.
        reason: &'static str,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange {
                code,
                cardinality,
                domain,
            } => write!(
                formatter,
                "code {code} is outside {domain} cardinality {cardinality}"
            ),
            Self::InvalidRefinement { refinement, reason } => {
                write!(formatter, "invalid {refinement}: {reason}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

fn out_of_range<T: FiniteDomain>(code: u128, domain: &'static str) -> DecodeError {
    DecodeError::OutOfRange {
        code,
        cardinality: T::cardinality(),
        domain,
    }
}

/// An inclusive bounded integer refinement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedU8<const MIN: u8, const MAX: u8>(u8);

impl<const MIN: u8, const MAX: u8> BoundedU8<MIN, MAX> {
    /// Construct after validating the inclusive bounds.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error when the value is outside the
    /// compile-time inclusive bounds.
    pub fn new(value: u8) -> Result<Self, DecodeError> {
        if MIN <= MAX && value >= MIN && value <= MAX {
            Ok(Self(value))
        } else {
            Err(DecodeError::InvalidRefinement {
                refinement: "BoundedU8",
                reason: "value is outside inclusive bounds",
            })
        }
    }

    /// Return the validated integer.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl<const MIN: u8, const MAX: u8> FiniteDomain for BoundedU8<MIN, MAX> {
    fn cardinality() -> u128 {
        u128::from(MAX.saturating_sub(MIN)) + u128::from(MIN <= MAX)
    }

    fn encode(&self) -> u128 {
        u128::from(self.0 - MIN)
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "BoundedU8"));
        }
        let offset = u8::try_from(code).expect("bounded-u8 cardinality fits u8");
        Ok(Self(MIN + offset))
    }
}

/// A validated supported Poche player count (2 through 51).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerCount(BoundedU8<2, 51>);

impl PlayerCount {
    /// Validate a player count.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error outside `2..=51`.
    pub fn new(value: u8) -> Result<Self, DecodeError> {
        BoundedU8::new(value).map(Self)
    }

    /// Return the count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl FiniteDomain for PlayerCount {
    fn cardinality() -> u128 {
        BoundedU8::<2, 51>::cardinality()
    }

    fn encode(&self) -> u128 {
        self.0.encode()
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        BoundedU8::decode(code).map(Self)
    }
}

/// A bid refined to `0..=HAND_SIZE`.
pub type Bid<const HAND_SIZE: u8> = BoundedU8<0, HAND_SIZE>;

/// Two-suit domain selected by exhaustive scope G4.
pub type MicroSuit = BoundedU8<0, 1>;

/// Three-rank domain selected by exhaustive scope G4.
pub type MicroRank = BoundedU8<0, 2>;

/// One of the six canonical cards in exhaustive scope G4.
pub type MicroCard = Pair<MicroSuit, MicroRank>;

/// Phase component for the selected exhaustive state encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MicroPhase {
    /// Chance must provide a deal.
    AwaitingDeal,
    /// Players are bidding.
    Bidding,
    /// Players are playing tricks.
    Playing,
    /// The environment must score and settle the round.
    Scoring,
    /// The game is terminal.
    Finished,
}

impl FiniteDomain for MicroPhase {
    fn cardinality() -> u128 {
        5
    }

    fn encode(&self) -> u128 {
        match self {
            Self::AwaitingDeal => 0,
            Self::Bidding => 1,
            Self::Playing => 2,
            Self::Scoring => 3,
            Self::Finished => 4,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::AwaitingDeal),
            1 => Ok(Self::Bidding),
            2 => Ok(Self::Playing),
            3 => Ok(Self::Scoring),
            4 => Ok(Self::Finished),
            _ => Err(out_of_range::<Self>(code, "MicroPhase")),
        }
    }
}

/// Turn-owner component for the selected exhaustive state encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MicroTurn {
    /// Explicit chance turn.
    Chance,
    /// First player acts.
    Player0,
    /// Second player acts.
    Player1,
    /// Deterministic environment settlement.
    Environment,
    /// No actor exists after termination.
    Finished,
}

impl FiniteDomain for MicroTurn {
    fn cardinality() -> u128 {
        5
    }

    fn encode(&self) -> u128 {
        match self {
            Self::Chance => 0,
            Self::Player0 => 1,
            Self::Player1 => 2,
            Self::Environment => 3,
            Self::Finished => 4,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::Chance),
            1 => Ok(Self::Player0),
            2 => Ok(Self::Player1),
            3 => Ok(Self::Environment),
            4 => Ok(Self::Finished),
            _ => Err(out_of_range::<Self>(code, "MicroTurn")),
        }
    }
}

/// Action component for G4. Bid and card payloads are included directly so
/// every legal or illegal candidate action has a canonical code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MicroAction {
    /// Provide the finite deal selected by chance.
    Deal,
    /// Bid zero.
    Bid0,
    /// Bid one.
    Bid1,
    /// Bid two.
    Bid2,
    /// Play one of six micro-card identities.
    Play(MicroCardCode),
    /// Settle the completed round.
    Settle,
}

/// Dense micro-card identity used in [`MicroAction::Play`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MicroCardCode(BoundedU8<0, 5>);

impl MicroCardCode {
    /// Validate a micro-card code.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error outside `0..=5`.
    pub fn new(code: u8) -> Result<Self, DecodeError> {
        BoundedU8::new(code).map(Self)
    }

    /// Return the dense identity.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl FiniteDomain for MicroCardCode {
    fn cardinality() -> u128 {
        6
    }

    fn encode(&self) -> u128 {
        self.0.encode()
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        BoundedU8::decode(code).map(Self)
    }
}

impl FiniteDomain for MicroAction {
    fn cardinality() -> u128 {
        11
    }

    fn encode(&self) -> u128 {
        match self {
            Self::Deal => 0,
            Self::Bid0 => 1,
            Self::Bid1 => 2,
            Self::Bid2 => 3,
            Self::Play(card) => 4 + card.encode(),
            Self::Settle => 10,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::Deal),
            1 => Ok(Self::Bid0),
            2 => Ok(Self::Bid1),
            3 => Ok(Self::Bid2),
            4..=9 => Ok(Self::Play(MicroCardCode::decode(code - 4)?)),
            10 => Ok(Self::Settle),
            _ => Err(out_of_range::<Self>(code, "MicroAction")),
        }
    }
}

/// Possible raw per-player score events in a two-card G4 round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MicroRoundScore {
    /// Missed bid: zero points.
    Miss,
    /// Exact zero bid: ten points.
    ExactZero,
    /// Exact one bid: eleven points.
    ExactOne,
    /// Exact all-tricks bid of two: twenty-two points.
    AllTwo,
}

impl MicroRoundScore {
    /// Numeric rulebook score.
    #[must_use]
    pub const fn points(self) -> u8 {
        match self {
            Self::Miss => 0,
            Self::ExactZero => 10,
            Self::ExactOne => 11,
            Self::AllTwo => 22,
        }
    }
}

impl FiniteDomain for MicroRoundScore {
    fn cardinality() -> u128 {
        4
    }

    fn encode(&self) -> u128 {
        match self {
            Self::Miss => 0,
            Self::ExactZero => 1,
            Self::ExactOne => 2,
            Self::AllTwo => 3,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::Miss),
            1 => Ok(Self::ExactZero),
            2 => Ok(Self::ExactOne),
            3 => Ok(Self::AllTwo),
            _ => Err(out_of_range::<Self>(code, "MicroRoundScore")),
        }
    }
}

/// A finite zero-based index refined to `0..N`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FiniteIndex<const N: usize>(usize);

impl<const N: usize> FiniteIndex<N> {
    /// Validate an index.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error when `index >= N`.
    pub fn new(index: usize) -> Result<Self, DecodeError> {
        if index < N {
            Ok(Self(index))
        } else {
            Err(DecodeError::InvalidRefinement {
                refinement: "FiniteIndex",
                reason: "index is outside the finite collection",
            })
        }
    }

    /// Return the index.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl<const N: usize> FiniteDomain for FiniteIndex<N> {
    fn cardinality() -> u128 {
        N as u128
    }

    fn encode(&self) -> u128 {
        self.0 as u128
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "FiniteIndex"));
        }
        Ok(Self(
            usize::try_from(code).expect("code is below usize-sized N"),
        ))
    }
}

/// Canonical finite `Option<T>` encoding (`None` first, then `Some`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Optional<T>(pub Option<T>);

impl<T: FiniteDomain> FiniteDomain for Optional<T> {
    fn cardinality() -> u128 {
        T::cardinality() + 1
    }

    fn encode(&self) -> u128 {
        self.0.as_ref().map_or(0, |value| value.encode() + 1)
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self(None)),
            value if value < Self::cardinality() => Ok(Self(Some(T::decode(value - 1)?))),
            _ => Err(out_of_range::<Self>(code, "Optional")),
        }
    }
}

/// Canonical mixed-radix pair encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair<A, B>(pub A, pub B);

impl<A: FiniteDomain, B: FiniteDomain> FiniteDomain for Pair<A, B> {
    fn cardinality() -> u128 {
        A::cardinality()
            .checked_mul(B::cardinality())
            .expect("pair cardinality fits u128")
    }

    fn encode(&self) -> u128 {
        self.0.encode() * B::cardinality() + self.1.encode()
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "Pair"));
        }
        Ok(Self(
            A::decode(code / B::cardinality())?,
            B::decode(code % B::cardinality())?,
        ))
    }
}

/// Canonical fixed-array mixed-radix encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedArray<T, const N: usize>(pub [T; N]);

impl<T: FiniteDomain, const N: usize> FiniteDomain for FixedArray<T, N> {
    fn cardinality() -> u128 {
        T::cardinality()
            .checked_pow(u32::try_from(N).expect("array length fits u32"))
            .expect("array cardinality fits u128")
    }

    fn encode(&self) -> u128 {
        self.0
            .iter()
            .fold(0, |code, value| code * T::cardinality() + value.encode())
    }

    fn decode(mut code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "FixedArray"));
        }
        let mut reversed = Vec::with_capacity(N);
        for _ in 0..N {
            reversed.push(T::decode(code % T::cardinality())?);
            code /= T::cardinality();
        }
        reversed.reverse();
        let values: [T; N] = reversed
            .try_into()
            .unwrap_or_else(|_| unreachable!("exactly N values were decoded"));
        Ok(Self(values))
    }
}

/// A canonical finite set encoded as the membership bit mask.
#[derive(Debug, PartialEq, Eq)]
pub struct FiniteSet<T> {
    bits: u128,
    marker: PhantomData<T>,
}

impl<T> Clone for FiniteSet<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for FiniteSet<T> {}

impl<T: FiniteDomain> FiniteSet<T> {
    /// Construct from values, deduplicating through the canonical bit mask.
    ///
    /// # Panics
    ///
    /// Panics when `T` has 128 or more values and cannot fit a `u128` bit set.
    pub fn from_values(values: impl IntoIterator<Item = T>) -> Self {
        assert!(
            T::cardinality() < 128,
            "finite-set domain must fit u128 bits"
        );
        let mut bits = 0;
        for value in values {
            bits |= 1_u128 << value.encode();
        }
        Self {
            bits,
            marker: PhantomData,
        }
    }

    /// Test semantic membership.
    #[must_use]
    pub fn contains(&self, value: &T) -> bool {
        self.bits & (1_u128 << value.encode()) != 0
    }
}

impl<T: FiniteDomain> FiniteDomain for FiniteSet<T> {
    fn cardinality() -> u128 {
        assert!(
            T::cardinality() < 128,
            "finite-set domain must fit u128 bits"
        );
        1_u128 << T::cardinality()
    }

    fn encode(&self) -> u128 {
        self.bits
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "FiniteSet"));
        }
        Ok(Self {
            bits: code,
            marker: PhantomData,
        })
    }
}

/// The four standard unranked suits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Suit {
    /// Clubs.
    Clubs,
    /// Diamonds.
    Diamonds,
    /// Hearts.
    Hearts,
    /// Spades.
    Spades,
}

impl FiniteDomain for Suit {
    fn cardinality() -> u128 {
        4
    }

    fn encode(&self) -> u128 {
        match self {
            Self::Clubs => 0,
            Self::Diamonds => 1,
            Self::Hearts => 2,
            Self::Spades => 3,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::Clubs),
            1 => Ok(Self::Diamonds),
            2 => Ok(Self::Hearts),
            3 => Ok(Self::Spades),
            _ => Err(out_of_range::<Self>(code, "Suit")),
        }
    }
}

/// A rank encoded as 2 through Ace (14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rank(BoundedU8<2, 14>);

impl Rank {
    /// Validate a rulebook rank.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error outside rank `2..=14`.
    pub fn new(value: u8) -> Result<Self, DecodeError> {
        BoundedU8::new(value).map(Self)
    }

    /// Return the numeric order value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl FiniteDomain for Rank {
    fn cardinality() -> u128 {
        13
    }

    fn encode(&self) -> u128 {
        self.0.encode()
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        BoundedU8::decode(code).map(Self)
    }
}

/// A canonical standard-card identity (`suit * 13 + rank`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardId(u8);

/// Stable failure for a non-canonical human card name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardNameError;

impl fmt::Display for CardNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("card must be rank-suit, for example jack-spades")
    }
}

impl std::error::Error for CardNameError {}

impl CardId {
    /// Construct from suit and rank.
    ///
    /// # Panics
    ///
    /// This function has no reachable panic for validated [`Suit`] and
    /// [`Rank`] values; the checked conversion documents that invariant.
    #[must_use]
    pub fn new(suit: Suit, rank: Rank) -> Self {
        Self(u8::try_from(suit.encode() * 13 + rank.encode()).expect("card code fits u8"))
    }

    /// Return the card's suit.
    ///
    /// # Panics
    ///
    /// This function has no reachable panic for a constructed [`CardId`].
    #[must_use]
    pub fn suit(self) -> Suit {
        Suit::decode(u128::from(self.0 / 13)).expect("validated card suit")
    }

    /// Return the card's rank.
    ///
    /// # Panics
    ///
    /// This function has no reachable panic for a constructed [`CardId`].
    #[must_use]
    pub fn rank(self) -> Rank {
        Rank::decode(u128::from(self.0 % 13)).expect("validated card rank")
    }

    /// Return the canonical dense standard-deck code.
    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }
}

/// Parse an exact lowercase `rank-suit` card name such as `jack-spades`.
///
/// # Errors
///
/// Rejects abbreviations, case variants, unknown values, and extra separators
/// so CLI/help/replay text has one canonical spelling.
pub fn parse_card_name(value: &str) -> Result<CardId, CardNameError> {
    let (rank, suit) = value.split_once('-').ok_or(CardNameError)?;
    if suit.contains('-') {
        return Err(CardNameError);
    }
    let rank = match rank {
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "jack" => 11,
        "queen" => 12,
        "king" => 13,
        "ace" => 14,
        _ => return Err(CardNameError),
    };
    let suit = match suit {
        "clubs" => Suit::Clubs,
        "diamonds" => Suit::Diamonds,
        "hearts" => Suit::Hearts,
        "spades" => Suit::Spades,
        _ => return Err(CardNameError),
    };
    Ok(CardId::new(
        suit,
        Rank::new(rank).map_err(|_| CardNameError)?,
    ))
}

impl FiniteDomain for CardId {
    fn cardinality() -> u128 {
        52
    }

    fn encode(&self) -> u128 {
        u128::from(self.0)
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "CardId"));
        }
        Ok(Self(u8::try_from(code).expect("card code is below 52")))
    }
}

/// A fixed sequence of distinct standard cards, densely ranked as a partial
/// permutation rather than accepting duplicate-card encodings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniqueCards<const N: usize>([CardId; N]);

impl<const N: usize> UniqueCards<N> {
    /// Validate uniqueness.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error for more than 52 entries or any
    /// duplicate card.
    pub fn new(cards: [CardId; N]) -> Result<Self, DecodeError> {
        if N > 52 {
            return Err(DecodeError::InvalidRefinement {
                refinement: "UniqueCards",
                reason: "more cards requested than exist in the deck",
            });
        }
        for (index, card) in cards.iter().enumerate() {
            if cards[..index].contains(card) {
                return Err(DecodeError::InvalidRefinement {
                    refinement: "UniqueCards",
                    reason: "duplicate card",
                });
            }
        }
        Ok(Self(cards))
    }

    /// Return the distinct cards.
    #[must_use]
    pub const fn cards(&self) -> &[CardId; N] {
        &self.0
    }
}

impl<const N: usize> FiniteDomain for UniqueCards<N> {
    fn cardinality() -> u128 {
        assert!(N <= 52, "unique-card sequence cannot exceed the deck");
        (0..N).fold(1_u128, |total, index| total * (52 - index) as u128)
    }

    fn encode(&self) -> u128 {
        let mut available: Vec<CardId> = CardId::enumerate();
        let mut code = 0_u128;
        for (index, card) in self.0.iter().enumerate() {
            let position = available
                .iter()
                .position(|candidate| candidate == card)
                .expect("validated cards occur exactly once");
            code = code * (52 - index) as u128 + position as u128;
            available.remove(position);
        }
        code
    }

    fn decode(mut code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "UniqueCards"));
        }
        let mut positions = Vec::with_capacity(N);
        for index in (0..N).rev() {
            let radix = (52 - index) as u128;
            positions.push(usize::try_from(code % radix).expect("position fits usize"));
            code /= radix;
        }
        positions.reverse();
        let mut available: Vec<CardId> = CardId::enumerate();
        let mut cards = Vec::with_capacity(N);
        for position in positions {
            cards.push(available.remove(position));
        }
        let cards: [CardId; N] = cards
            .try_into()
            .unwrap_or_else(|_| unreachable!("exactly N cards were decoded"));
        Ok(Self(cards))
    }
}

/// Zone assignment in the G4 exhaustive micro-scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MicroZone {
    /// First player's hand.
    Player0,
    /// Second player's hand.
    Player1,
    /// Revealed trump card.
    Trump,
    /// Single undealt card.
    Undealt,
}

impl MicroZone {
    const fn wire_code(self) -> u8 {
        match self {
            Self::Player0 => 0,
            Self::Player1 => 1,
            Self::Trump => 2,
            Self::Undealt => 3,
        }
    }
}

impl FiniteDomain for MicroZone {
    fn cardinality() -> u128 {
        4
    }

    fn encode(&self) -> u128 {
        match self {
            Self::Player0 => 0,
            Self::Player1 => 1,
            Self::Trump => 2,
            Self::Undealt => 3,
        }
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        match code {
            0 => Ok(Self::Player0),
            1 => Ok(Self::Player1),
            2 => Ok(Self::Trump),
            3 => Ok(Self::Undealt),
            _ => Err(out_of_range::<Self>(code, "MicroZone")),
        }
    }
}

/// A complete six-card partition for G4: two cards per player, one trump, and
/// one undealt card. Dense enumeration contains exactly 180 partitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicroPartition([MicroZone; 6]);

impl MicroPartition {
    /// Validate the required per-zone cardinalities.
    ///
    /// # Errors
    ///
    /// Returns an invalid-refinement error unless zone counts are `[2,2,1,1]`.
    pub fn new(zones: [MicroZone; 6]) -> Result<Self, DecodeError> {
        let counts = MicroZone::enumerate()
            .into_iter()
            .map(|zone| zones.iter().filter(|candidate| **candidate == zone).count());
        let counts: Vec<usize> = counts.collect();
        if counts == [2, 2, 1, 1] {
            Ok(Self(zones))
        } else {
            Err(DecodeError::InvalidRefinement {
                refinement: "MicroPartition",
                reason: "expected zone counts [2,2,1,1]",
            })
        }
    }

    /// Return the zone for each canonical micro-card id.
    #[must_use]
    pub const fn zones(&self) -> &[MicroZone; 6] {
        &self.0
    }

    fn raw_partitions() -> impl Iterator<Item = Self> {
        FixedArray::<MicroZone, 6>::enumerate()
            .into_iter()
            .filter_map(|raw| Self::new(raw.0).ok())
    }
}

impl FiniteDomain for MicroPartition {
    fn cardinality() -> u128 {
        180
    }

    fn encode(&self) -> u128 {
        Self::raw_partitions()
            .position(|candidate| candidate == *self)
            .expect("validated partition is enumerated") as u128
    }

    fn decode(code: u128) -> Result<Self, DecodeError> {
        if code >= Self::cardinality() {
            return Err(out_of_range::<Self>(code, "MicroPartition"));
        }
        Self::raw_partitions()
            .nth(usize::try_from(code).expect("partition code fits usize"))
            .ok_or_else(|| out_of_range::<Self>(code, "MicroPartition"))
    }
}

/// Facet/Phon wire shape for the selected exhaustive scope. Semantic wrappers
/// are reconstructed with `TryFrom`, never trusted merely because bytes decode.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SelectedScopeWire {
    /// Raw player-count shape.
    pub player_count: u8,
    /// Raw bid shape for the two-card scope.
    pub bid: u8,
    /// Raw standard-card identities.
    pub distinct_cards: [u8; 2],
    /// Raw G4 zone discriminants.
    pub partition: [u8; 6],
}

/// Semantically refined selected-scope components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedScopeFixture {
    /// Supported player count.
    pub player_count: PlayerCount,
    /// Bid bounded by the G4 two-card hand.
    pub bid: Bid<2>,
    /// Two distinct standard cards.
    pub distinct_cards: UniqueCards<2>,
    /// Complete G4 card partition.
    pub partition: MicroPartition,
}

impl From<&SelectedScopeFixture> for SelectedScopeWire {
    fn from(value: &SelectedScopeFixture) -> Self {
        Self {
            player_count: value.player_count.get(),
            bid: value.bid.get(),
            distinct_cards: value.distinct_cards.0.map(|card| card.0),
            partition: value.partition.0.map(MicroZone::wire_code),
        }
    }
}

impl TryFrom<SelectedScopeWire> for SelectedScopeFixture {
    type Error = DecodeError;

    fn try_from(value: SelectedScopeWire) -> Result<Self, Self::Error> {
        let distinct_cards = value
            .distinct_cards
            .map(|card| CardId::decode(u128::from(card)))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap_or_else(|_| unreachable!("array map retains length"));
        let partition = value
            .partition
            .map(|zone| MicroZone::decode(u128::from(zone)))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap_or_else(|_| unreachable!("array map retains length"));
        Ok(Self {
            player_count: PlayerCount::new(value.player_count)?,
            bid: Bid::new(value.bid)?,
            distinct_cards: UniqueCards::new(distinct_cards)?,
            partition: MicroPartition::new(partition)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_roundtrip<T: FiniteDomain + fmt::Debug>() {
        for value in T::enumerate() {
            assert_eq!(T::decode(value.encode()), Ok(value));
        }
        assert!(T::decode(T::cardinality()).is_err());
    }

    #[test]
    fn finite_domain_enumeration_and_spare_codes() {
        assert_roundtrip::<Suit>();
        assert_roundtrip::<Rank>();
        assert_roundtrip::<CardId>();
        assert_roundtrip::<Bid<2>>();
        assert_roundtrip::<FiniteIndex<3>>();
        assert_roundtrip::<Optional<Suit>>();
        assert_roundtrip::<Pair<Suit, Bid<2>>>();
        assert_roundtrip::<FixedArray<Bid<1>, 3>>();
        assert_roundtrip::<FiniteSet<Bid<2>>>();
        assert_roundtrip::<MicroCard>();
        assert_roundtrip::<MicroPhase>();
        assert_roundtrip::<MicroTurn>();
        assert_roundtrip::<MicroAction>();
        assert_roundtrip::<MicroRoundScore>();
        assert_eq!(CardId::bit_width(), 6);
        assert_eq!(CardId::enumerate().len(), 52);
    }

    #[test]
    fn canonical_card_name_parsing_is_exact_and_dense() {
        assert_eq!(parse_card_name("two-clubs").unwrap().code(), 0);
        assert_eq!(parse_card_name("jack-spades").unwrap().code(), 48);
        assert_eq!(parse_card_name("ace-spades").unwrap().code(), 51);
        for rejected in ["J-spades", "jack-spade", "jack--spades", " jack-spades"] {
            assert!(parse_card_name(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn refinement_unique_cards_and_partitions_are_dense() {
        let cards = [CardId::decode(0).unwrap(), CardId::decode(51).unwrap()];
        let unique = UniqueCards::new(cards).unwrap();
        assert_eq!(UniqueCards::<2>::decode(unique.encode()), Ok(unique));
        assert!(UniqueCards::new([cards[0], cards[0]]).is_err());
        assert_eq!(UniqueCards::<2>::cardinality(), 52 * 51);

        assert_roundtrip::<MicroPartition>();
        assert_eq!(MicroPartition::enumerate().len(), 180);
        assert!(MicroPartition::new([MicroZone::Player0; 6]).is_err());
    }

    #[test]
    fn refinement_rejects_invalid_shape_values() {
        assert!(PlayerCount::new(1).is_err());
        assert!(PlayerCount::new(52).is_err());
        assert!(Bid::<2>::new(3).is_err());
        assert!(FiniteIndex::<2>::new(2).is_err());

        let invalid = SelectedScopeWire {
            player_count: 2,
            bid: 2,
            distinct_cards: [7, 7],
            partition: [0, 0, 1, 1, 2, 3],
        };
        assert!(SelectedScopeFixture::try_from(invalid).is_err());
    }

    #[test]
    fn phon_roundtrip_revalidates_semantics() {
        let fixture = SelectedScopeFixture {
            player_count: PlayerCount::new(2).unwrap(),
            bid: Bid::new(2).unwrap(),
            distinct_cards: UniqueCards::new([
                CardId::new(Suit::Clubs, Rank::new(2).unwrap()),
                CardId::new(Suit::Spades, Rank::new(14).unwrap()),
            ])
            .unwrap(),
            partition: MicroPartition::new([
                MicroZone::Player0,
                MicroZone::Player0,
                MicroZone::Player1,
                MicroZone::Player1,
                MicroZone::Trump,
                MicroZone::Undealt,
            ])
            .unwrap(),
        };
        let wire = SelectedScopeWire::from(&fixture);
        let bytes = phon::api::encode(&wire).unwrap();
        let decoded: SelectedScopeWire = phon::api::decode(&bytes).unwrap();
        assert_eq!(SelectedScopeFixture::try_from(decoded), Ok(fixture));
    }
}

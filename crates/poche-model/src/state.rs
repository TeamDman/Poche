use facet::Facet;

use crate::{Bid, Card, CardSet, Deal, Player, Pot, RoundId, Score, Tricks};

/// Coarse semantic phase, derived from the phase-specific state variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Phase {
    /// Chance supplies a complete deal partition.
    AwaitingDeal,
    /// Players announce fixed bids.
    Bidding,
    /// Players play one card per trick.
    Playing,
    /// Environment scores and advances.
    Scoring,
    /// Absorbing terminal phase.
    Finished,
}

/// Exact action owner at the environment boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TurnOwner {
    /// Explicit chance choice.
    Chance,
    /// One player chooses a legal action.
    Player(Player),
    /// Deterministic environment settlement.
    Environment,
    /// No action after termination.
    Finished,
}

/// Ledger shared by every nonterminal phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Ledger {
    pub(crate) dealer: Player,
    pub(crate) round: RoundId,
    pub(crate) scores: [Score; 2],
    pub(crate) pot: Pot,
}

impl Ledger {
    /// Current dealer.
    #[must_use]
    pub const fn dealer(self) -> Player {
        self.dealer
    }

    /// Scheduled round.
    #[must_use]
    pub const fn round(self) -> RoundId {
        self.round
    }

    /// Cumulative scores.
    #[must_use]
    pub const fn scores(self) -> [Score; 2] {
        self.scores
    }

    /// Communal pot.
    #[must_use]
    pub const fn pot(self) -> Pot {
        self.pot
    }
}

/// Chance-owned state before each round deal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct AwaitingDeal {
    pub(crate) ledger: Ledger,
}

impl AwaitingDeal {
    /// Shared ledger.
    #[must_use]
    pub const fn ledger(self) -> Ledger {
        self.ledger
    }
}

/// Structurally exact bidding progress for two clockwise bids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum BidProgress {
    /// Dealer's left neighbor has not bid yet.
    First,
    /// First bidder's value is fixed; dealer bids last.
    DealerLast(Bid),
}

/// Bidding state with a complete fixed card partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Bidding {
    pub(crate) ledger: Ledger,
    pub(crate) hands: [CardSet; 2],
    pub(crate) trump: Card,
    pub(crate) undealt: CardSet,
    pub(crate) progress: BidProgress,
}

impl Bidding {
    /// Shared ledger.
    #[must_use]
    pub const fn ledger(self) -> Ledger {
        self.ledger
    }

    /// Current actor derived from bid progress.
    #[must_use]
    pub const fn actor(self) -> Player {
        match self.progress {
            BidProgress::First => self.ledger.dealer.left(),
            BidProgress::DealerLast(_) => self.ledger.dealer,
        }
    }

    /// Private hand for `player`.
    #[must_use]
    pub const fn hand(self, player: Player) -> CardSet {
        self.hands[player.index()]
    }

    /// Revealed trump card.
    #[must_use]
    pub const fn trump(self) -> Card {
        self.trump
    }

    /// Public fixed bids made so far.
    #[must_use]
    pub const fn bids(self) -> [Option<Bid>; 2] {
        match self.progress {
            BidProgress::First => [None, None],
            BidProgress::DealerLast(bid) => {
                let mut bids = [None, None];
                bids[self.ledger.dealer.left().index()] = Some(bid);
                bids
            }
        }
    }
}

/// One public card play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct PlayedCard {
    /// Playing seat.
    pub player: Player,
    /// Played card.
    pub card: Card,
}

/// Current-trick progress. A complete two-card trick is resolved immediately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TrickProgress {
    /// Leader is ready to establish the lead suit.
    Lead(Player),
    /// One lead card is public and the other player follows.
    Follow(PlayedCard),
}

impl TrickProgress {
    /// Current actor.
    #[must_use]
    pub const fn actor(self) -> Player {
        match self {
            Self::Lead(player) => player,
            Self::Follow(lead) => lead.player.left(),
        }
    }

    /// Current leader.
    #[must_use]
    pub const fn leader(self) -> Player {
        match self {
            Self::Lead(player) => player,
            Self::Follow(lead) => lead.player,
        }
    }

    /// Public lead card, when already played.
    #[must_use]
    pub const fn lead(self) -> Option<PlayedCard> {
        match self {
            Self::Lead(_) => None,
            Self::Follow(lead) => Some(lead),
        }
    }
}

/// Strict trick-play state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Playing {
    pub(crate) ledger: Ledger,
    pub(crate) hands: [CardSet; 2],
    pub(crate) trump: Card,
    pub(crate) undealt: CardSet,
    pub(crate) bids: [Bid; 2],
    pub(crate) trick: TrickProgress,
    pub(crate) captured: [CardSet; 2],
    pub(crate) tricks_won: [Tricks; 2],
}

impl Playing {
    /// Shared ledger.
    #[must_use]
    pub const fn ledger(self) -> Ledger {
        self.ledger
    }

    /// Current actor.
    #[must_use]
    pub const fn actor(self) -> Player {
        self.trick.actor()
    }

    /// Current leader.
    #[must_use]
    pub const fn leader(self) -> Player {
        self.trick.leader()
    }

    /// Private hand for `player`.
    #[must_use]
    pub const fn hand(self, player: Player) -> CardSet {
        self.hands[player.index()]
    }

    /// Revealed trump card.
    #[must_use]
    pub const fn trump(self) -> Card {
        self.trump
    }

    /// Fixed bids.
    #[must_use]
    pub const fn bids(self) -> [Bid; 2] {
        self.bids
    }

    /// Current public trick.
    #[must_use]
    pub const fn trick(self) -> TrickProgress {
        self.trick
    }

    /// Trick counts by seat.
    #[must_use]
    pub const fn tricks_won(self) -> [Tricks; 2] {
        self.tricks_won
    }
}

/// Deterministic round-settlement state. Hands and current trick are
/// structurally absent because all played cards are in captured piles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Scoring {
    pub(crate) ledger: Ledger,
    pub(crate) trump: Card,
    pub(crate) undealt: CardSet,
    pub(crate) bids: [Bid; 2],
    pub(crate) captured: [CardSet; 2],
    pub(crate) tricks_won: [Tricks; 2],
}

impl Scoring {
    /// Shared ledger.
    #[must_use]
    pub const fn ledger(self) -> Ledger {
        self.ledger
    }

    /// Fixed bids.
    #[must_use]
    pub const fn bids(self) -> [Bid; 2] {
        self.bids
    }

    /// Final round trick counts.
    #[must_use]
    pub const fn tricks_won(self) -> [Tricks; 2] {
        self.tricks_won
    }
}

/// Terminal state with score and pot projections kept separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Finished {
    pub(crate) scores: [Score; 2],
    pub(crate) pot: Pot,
    pub(crate) winners: [bool; 2],
}

impl Finished {
    /// Final cumulative scores.
    #[must_use]
    pub const fn scores(self) -> [Score; 2] {
        self.scores
    }

    /// Final communal pot.
    #[must_use]
    pub const fn pot(self) -> Pot {
        self.pot
    }

    /// Complete maximum-score tie mask.
    #[must_use]
    pub const fn winners(self) -> [bool; 2] {
        self.winners
    }
}

/// Phase-specific complete semantic game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Game {
    /// Chance-owned deal boundary.
    AwaitingDeal(AwaitingDeal),
    /// Player-owned bid phase.
    Bidding(Bidding),
    /// Player-owned trick phase.
    Playing(Playing),
    /// Environment-owned deterministic scoring phase.
    Scoring(Scoring),
    /// Absorbing terminal phase.
    Finished(Finished),
}

impl Game {
    /// Prepare a micro-game after the first dealer has been explicitly chosen.
    #[must_use]
    pub const fn new(first_dealer: Player) -> Self {
        Self::AwaitingDeal(AwaitingDeal {
            ledger: Ledger {
                dealer: first_dealer,
                round: RoundId::OneAscending,
                scores: [Score::ZERO; 2],
                pot: Pot::OPENING,
            },
        })
    }

    /// Coarse phase derived without nullable fields.
    #[must_use]
    pub const fn phase(self) -> Phase {
        match self {
            Self::AwaitingDeal(_) => Phase::AwaitingDeal,
            Self::Bidding(_) => Phase::Bidding,
            Self::Playing(_) => Phase::Playing,
            Self::Scoring(_) => Phase::Scoring,
            Self::Finished(_) => Phase::Finished,
        }
    }

    /// Exact action owner.
    #[must_use]
    pub const fn turn(self) -> TurnOwner {
        match self {
            Self::AwaitingDeal(_) => TurnOwner::Chance,
            Self::Bidding(state) => TurnOwner::Player(state.actor()),
            Self::Playing(state) => TurnOwner::Player(state.actor()),
            Self::Scoring(_) => TurnOwner::Environment,
            Self::Finished(_) => TurnOwner::Finished,
        }
    }

    /// Project exactly one viewer's information.
    #[must_use]
    pub fn observe(self, viewer: Player) -> Observation {
        match self {
            Self::AwaitingDeal(state) => Observation::empty(
                Phase::AwaitingDeal,
                viewer,
                Some(state.ledger),
                TurnOwner::Chance,
            ),
            Self::Bidding(state) => Observation {
                phase: Phase::Bidding,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: TurnOwner::Player(state.actor()),
                round: Some(state.ledger.round),
                private_hand: state.hands[viewer.index()],
                hand_counts: state.hands.map(CardSet::len),
                trump: Some(state.trump),
                current_trick: None,
                bids: state.bids(),
                tricks_won: [Tricks::default(); 2],
                scores: state.ledger.scores,
                pot: state.ledger.pot,
            },
            Self::Playing(state) => Observation {
                phase: Phase::Playing,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: TurnOwner::Player(state.actor()),
                round: Some(state.ledger.round),
                private_hand: state.hands[viewer.index()],
                hand_counts: state.hands.map(CardSet::len),
                trump: Some(state.trump),
                current_trick: state.trick.lead(),
                bids: state.bids.map(Some),
                tricks_won: state.tricks_won,
                scores: state.ledger.scores,
                pot: state.ledger.pot,
            },
            Self::Scoring(state) => Observation {
                phase: Phase::Scoring,
                viewer,
                dealer: Some(state.ledger.dealer),
                actor: TurnOwner::Environment,
                round: Some(state.ledger.round),
                private_hand: CardSet::EMPTY,
                hand_counts: [0, 0],
                trump: Some(state.trump),
                current_trick: None,
                bids: state.bids.map(Some),
                tricks_won: state.tricks_won,
                scores: state.ledger.scores,
                pot: state.ledger.pot,
            },
            Self::Finished(state) => Observation {
                phase: Phase::Finished,
                viewer,
                dealer: None,
                actor: TurnOwner::Finished,
                round: None,
                private_hand: CardSet::EMPTY,
                hand_counts: [0, 0],
                trump: None,
                current_trick: None,
                bids: [None, None],
                tricks_won: [Tricks::default(); 2],
                scores: state.scores,
                pot: state.pot,
            },
        }
    }

    /// Enumerate choices owned by the current actor.
    #[must_use]
    pub fn legal_actions(self) -> LegalActions {
        match self {
            Self::AwaitingDeal(state) => LegalActions::Chance(
                Deal::all_for(state.ledger.round)
                    .into_iter()
                    .map(ChanceAction::Deal)
                    .collect(),
            ),
            Self::Bidding(state) => LegalActions::Player(
                Bid::ALL
                    .into_iter()
                    .filter(|bid| {
                        crate::formal::bid_legal(*bid, state.ledger.round).unwrap_or(false)
                    })
                    .map(|bid| PlayerAction::Bid {
                        player: state.actor(),
                        bid,
                    })
                    .collect(),
            ),
            Self::Playing(state) => {
                let actor = state.actor();
                let hand = state.hands[actor.index()];
                let lead = state.trick.lead();
                LegalActions::Player(
                    hand.iter()
                        .filter(|card| {
                            lead.is_none_or(|lead| {
                                crate::formal::follow_suit_legal(
                                    hand.contains_suit(lead.card.suit()),
                                    *card,
                                    lead.card,
                                )
                                .unwrap_or(false)
                            })
                        })
                        .map(|card| PlayerAction::Play {
                            player: actor,
                            card,
                        })
                        .collect(),
                )
            }
            Self::Scoring(_) => LegalActions::Environment,
            Self::Finished(_) => LegalActions::Finished,
        }
    }
}

/// Information visible to one player. No field can contain another player's
/// private hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct Observation {
    /// Coarse phase.
    pub phase: Phase,
    /// Viewing player.
    pub viewer: Player,
    /// Public dealer, absent after finish.
    pub dealer: Option<Player>,
    /// Exact action owner.
    pub actor: TurnOwner,
    /// Scheduled round, absent after finish.
    pub round: Option<RoundId>,
    /// Only the viewer's private cards.
    pub private_hand: CardSet,
    /// Public card counts by seat.
    pub hand_counts: [u8; 2],
    /// Public revealed trump card.
    pub trump: Option<Card>,
    /// Public lead card, if the trick is awaiting its follower.
    pub current_trick: Option<PlayedCard>,
    /// Public fixed bids announced so far.
    pub bids: [Option<Bid>; 2],
    /// Public trick counts.
    pub tricks_won: [Tricks; 2],
    /// Public cumulative scores.
    pub scores: [Score; 2],
    /// Public communal pot.
    pub pot: Pot,
}

impl Observation {
    fn empty(phase: Phase, viewer: Player, ledger: Option<Ledger>, actor: TurnOwner) -> Self {
        Self {
            phase,
            viewer,
            dealer: ledger.map(Ledger::dealer),
            actor,
            round: ledger.map(Ledger::round),
            private_hand: CardSet::EMPTY,
            hand_counts: [0, 0],
            trump: None,
            current_trick: None,
            bids: [None, None],
            tricks_won: [Tricks::default(); 2],
            scores: ledger.map_or([Score::default(); 2], Ledger::scores),
            pot: ledger.map_or(Pot::OPENING, Ledger::pot),
        }
    }
}

/// Player-policy action; chance and settlement cannot be represented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PlayerAction {
    /// Announce a fixed bid.
    Bid {
        /// Acting player.
        player: Player,
        /// Whole-number trick bid.
        bid: Bid,
    },
    /// Play a card from the acting player's hand.
    Play {
        /// Acting player.
        player: Player,
        /// Selected card.
        card: Card,
    },
}

/// Explicit chance action; a random generator is outside semantic state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChanceAction {
    /// Supply one of the 180 exact card partitions.
    Deal(Deal),
}

/// Complete legal choice surface with disjoint ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegalActions {
    /// Chance choices.
    Chance(Vec<ChanceAction>),
    /// Acting player's choices.
    Player(Vec<PlayerAction>),
    /// One deterministic settlement step.
    Environment,
    /// No action in the absorbing terminal state.
    Finished,
}

/// Conservative measurable raw-state bound before semantic invariants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateSpaceMeasure {
    /// Upper bound on raw structural encodings.
    pub upper_bound: u128,
    /// `false` because cross-field refinements make this bound conservative.
    pub exact: bool,
}

/// Return a finite upper bound for every phase-specific raw state shape.
#[must_use]
pub fn state_space_measure() -> StateSpaceMeasure {
    const PLAYERS: u128 = 2;
    const ROUNDS: u128 = 3;
    const SCORES: u128 = 65 * 65;
    const POTS: u128 = 7;
    const LEDGERS: u128 = PLAYERS * ROUNDS * SCORES * POTS;
    const CARD_SETS: u128 = 64;
    const CARDS: u128 = 6;
    const BIDS: u128 = 3;
    const TRICKS: u128 = 3;

    let awaiting = LEDGERS;
    let bidding = LEDGERS * CARD_SETS.pow(3) * CARDS * (1 + BIDS);
    let playing = LEDGERS
        * CARD_SETS.pow(5)
        * CARDS
        * BIDS.pow(2)
        * (PLAYERS + PLAYERS * CARDS)
        * TRICKS.pow(2);
    let scoring = LEDGERS * CARD_SETS.pow(3) * CARDS * BIDS.pow(2) * TRICKS.pow(2);
    let finished = SCORES * POTS * 4;
    StateSpaceMeasure {
        upper_bound: awaiting + bidding + playing + scoring + finished,
        exact: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_domain::FiniteDomain;

    fn first_deal(round: RoundId) -> Deal {
        Deal::all_for(round)[0]
    }

    fn bidding(round: RoundId, progress: BidProgress) -> Game {
        let deal = first_deal(round);
        Game::Bidding(Bidding {
            ledger: Ledger {
                dealer: Player::One,
                round,
                scores: [Score::default(); 2],
                pot: Pot::OPENING,
            },
            hands: deal.hands(),
            trump: deal.trump(),
            undealt: deal.undealt(),
            progress,
        })
    }

    #[test]
    fn state_domain_is_finite_and_phase_specific() {
        let measure = state_space_measure();
        assert!(measure.upper_bound > Deal::cardinality());
        assert!(!measure.exact);
        assert_eq!(RoundId::ALL.map(RoundId::hand_size), [1, 2, 1]);
        assert_eq!(Deal::all_for(RoundId::OneAscending).len(), 120);
        assert_eq!(Deal::all_for(RoundId::Two).len(), 180);
        assert_eq!(Game::new(Player::Zero).phase(), Phase::AwaitingDeal);
    }

    #[test]
    fn observation_schema_hides_other_private_hands() {
        let game = bidding(RoundId::Two, BidProgress::First);
        let Game::Bidding(state) = game else {
            unreachable!();
        };
        let zero = game.observe(Player::Zero);
        let one = game.observe(Player::One);
        assert_eq!(zero.private_hand, state.hand(Player::Zero));
        assert_eq!(one.private_hand, state.hand(Player::One));
        assert_ne!(zero.private_hand, one.private_hand);
        assert_eq!(zero.hand_counts, [2, 2]);
        assert_eq!(one.hand_counts, [2, 2]);

        let bytes = phon::api::encode(&zero).unwrap();
        assert_eq!(phon::api::decode::<Observation>(&bytes).unwrap(), zero);
    }

    #[test]
    fn action_enumeration_is_finite_and_owner_typed() {
        let LegalActions::Chance(deals) = Game::new(Player::Zero).legal_actions() else {
            unreachable!();
        };
        assert_eq!(deals.len(), 120);

        let LegalActions::Player(one_card_bids) =
            bidding(RoundId::OneAscending, BidProgress::First).legal_actions()
        else {
            unreachable!();
        };
        assert_eq!(one_card_bids.len(), 2);
        assert!(one_card_bids.contains(&PlayerAction::Bid {
            player: Player::Zero,
            bid: Bid::Zero,
        }));
        assert!(one_card_bids.contains(&PlayerAction::Bid {
            player: Player::Zero,
            bid: Bid::One,
        }));

        let LegalActions::Player(two_card_bids) =
            bidding(RoundId::Two, BidProgress::DealerLast(Bid::Two)).legal_actions()
        else {
            unreachable!();
        };
        assert_eq!(two_card_bids.len(), 3);
        assert!(two_card_bids.iter().all(|action| matches!(
            action,
            PlayerAction::Bid {
                player: Player::One,
                ..
            }
        )));
    }
}

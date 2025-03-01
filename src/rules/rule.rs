use crate::rules::ante::AnteBehaviour;
use crate::rules::bet::BetBehaviour;
use crate::rules::collect_bets::CollectBetsBehaviour;
use crate::rules::deal_card::DealCardBehaviour;
use crate::rules::deal_hands::DealHandsBehaviour;
use crate::rules::determine_bet_outcomes::DetermineBetOutcomesBehaviour;
use crate::rules::determine_dealer::DetermineDealerBehaviour;
use crate::rules::determine_trick_winner::DetermineTrickWinnerBehaviour;
use crate::rules::determine_winner::DetermineWinnerBehaviour;
use crate::rules::move_pile_to_winner::MovePileToWinnerBehaviour;
use crate::rules::next_round::NextRoundBehaviour;
use crate::rules::pass_dealer::PassDealerBehaviour;
use crate::rules::play_card::PlayCardBehaviour;
use crate::rules::play_round::PlayRoundBehaviour;
use crate::rules::play_trick::PlayTrickBehaviour;
use crate::rules::reveal_trump::RevealTrumpBehaviour;
use crate::rules::round_over::RoundOverBehaviour;
use crate::rules::shuffle::ShuffleBehaviour;
use crate::rules::update_bet_outcome::UpdateBetOutcomeBehaviour;
use crate::state::State;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Rule {
    /// Each player pays a quarter to the pot
    /// Push DetermineDealer
    Ante,

    /// The dealer is determined at random.
    /// If a previous game exists, the dealer continues to the next player.
    /// Push Shuffle
    DetermineDealer,

    /// The deck is shuffled.
    /// Push DealHands
    Shuffle,

    /// Push DealCard * num_players
    /// Push RevealTrump
    DealHands,

    /// The player clockwise from the dealer with the least cards receives a card.
    /// Dealer is the last player to receive a card.
    DealCard,

    /// The top card of the deck is flipped face up
    /// Push CollectBets
    RevealTrump,

    /// Push Bet * num_players
    /// Push PlayHand
    CollectBets,

    /// The player clockwise from the dealer who has not yet bet this round places a bet on how many tricks they think they will take.
    /// The dealer bets last.
    Bet,

    /// Push PlayTrick * hand_size
    /// The player to the left of the dealer becomes the active player
    /// Push RoundOver
    PlayRound,

    /// Push PlayCard * num_players
    /// Push DetermineTrickWinner
    PlayTrick,

    /// The active player plays a card from their hand to the top of the pile.
    /// The next player clockwise becomes the active player.
    /// The first card played in the trick determines the lead suit.
    /// Players must follow suit if able.
    PlayCard,

    /// All players have played a card for this trick.
    /// The pile should have a number of cards equal to the number of players.
    /// The player who played the highest card of the lead suit wins the trick.
    /// Trump cards beat all other suits.
    /// Push MovePileToWinner
    DetermineTrickWinner,

    /// The player who won the trick turns the cards face down and places them in front of themselves.
    /// The number of tricks a player has taken is indicated by the number of piles in front of them.
    MovePileToWinner,

    /// The round is over when all players have played all their cards.
    /// Push DetermineBetOutcomes
    RoundOver,

    /// Push UpdateBetOutcome * num_players
    /// Push NextRound
    DetermineBetOutcomes,

    /// If you took the number of cards you bet, you get 10+bet points.
    /// If you took all the tricks, and you bet to take all the tricks, you get 20+bet points.
    /// If you did not take the number of tricks you bet, you get 0 points.
    UpdateBetOutcome,

    /// The round is over, the next round begins.
    /// The number of cards dealt to each player increases by 1 each round until the 7th round.
    /// The number of cards dealt to each player decreases by 1 each round after the 7th round.
    /// If there isn't enough cards to deal everyone the required number, the hand size starts going down sooner than after the 7th round to accommodate.
    /// If the last round has just completed, push DetermineWinner
    /// If the last round has not completed, push PassDealer
    NextRound,

    /// The dealer passes to the next player clockwise.
    /// Push Shuffle
    PassDealer,

    /// The game is over when the last round is complete.
    /// The winner is the player with the most points.
    DetermineWinner,
}
pub trait RuleBehaviour {
    fn apply(&self, state: &mut State) -> eyre::Result<()>;
}
impl RuleBehaviour for Rule {
    fn apply(&self, state: &mut State) -> eyre::Result<()> {
        match self {
            Rule::Ante => AnteBehaviour.apply(state),
            Rule::DetermineDealer => DetermineDealerBehaviour.apply(state),
            Rule::Shuffle => ShuffleBehaviour.apply(state),
            Rule::DealHands => DealHandsBehaviour.apply(state),
            Rule::DealCard => DealCardBehaviour.apply(state),
            Rule::RevealTrump => RevealTrumpBehaviour.apply(state),
            Rule::CollectBets => CollectBetsBehaviour.apply(state),
            Rule::Bet => BetBehaviour.apply(state),
            Rule::PlayRound => PlayRoundBehaviour.apply(state),
            Rule::PlayTrick => PlayTrickBehaviour.apply(state),
            Rule::PlayCard => PlayCardBehaviour.apply(state),
            Rule::DetermineTrickWinner => DetermineTrickWinnerBehaviour.apply(state),
            Rule::MovePileToWinner => MovePileToWinnerBehaviour.apply(state),
            Rule::RoundOver => RoundOverBehaviour.apply(state),
            Rule::DetermineBetOutcomes => DetermineBetOutcomesBehaviour.apply(state),
            Rule::UpdateBetOutcome => UpdateBetOutcomeBehaviour.apply(state),
            Rule::NextRound => NextRoundBehaviour.apply(state),
            Rule::PassDealer => PassDealerBehaviour.apply(state),
            Rule::DetermineWinner => DetermineWinnerBehaviour.apply(state),
        }
    }
}

/*
4-player demo
Ante
DetermineDealer, - Player 3
Shuffle
DealHands - 1 card to each player
DealCard - player_0 - 5 of hearts
DealCard - player_1 - 6 of hearts
DealCard - player_2 - 3 of spades
DealCard - player_3 - 2 of clubs
RevealTrump - 4 of hearts
CollectBets
Bet - player_1 - 1
Bet - player_2 - 0
Bet - player_3 - 0
Bet - player_0 - 1
PlayTrick
PlayCard player_1 - 6 of hearts, hearts is now lead suit, players must play hearts this trick if able
PlayCard player_2 - 3 of spades
PlayCard player_3 - 2 of clubs
PlayCard player_0 - 5 of hearts
DetermineTrickWinner - 6 of hearts, player 1
RoundOver
DetermineBetOutcomes
UpdateBetOutcome - player_0 bet 1, becomes 0 (bet failed, zero points)
UpdateBetOutcome - player_1 bet 1, becomes 21 (+10 bet success, +10 took all tricks, base +1 from original bet of 1 trick)
UpdateBetOutcome - player_2 bet 0, becomes 10 (+10 bet success, base +0 bet of 0 tricks)
UpdateBetOutcome - player_3 bet 0, becomes 10 (+10 bet success, base +0 bet of 0 tricks)
NextRound - entering round 2, going up
PassDealer - Player 0
Shuffle
DealHands - 2 cards to each player
DealCard - player_1 - 4 of hearts
DealCard - player_2 - 4 of clubs
DealCard - player_3 - 8 of diamonds
DealCard - player_0 - king of spades
DealCard - player_1 - 7 of hearts
DealCard - player_2 - 5 of clubs
DealCard - player_3 - 9 of diamonds
DealCard - player_0 - 8 of spades
RevealTrump - 5 of hearts
CollectBets
Bet - player_2 - 0
Bet - player_3 - 0
Bet - player_0 - 0
Bet - player_1 - 2
PlayTrick




 */

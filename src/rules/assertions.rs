use crate::cards::Card;
use crate::cards::Deck;
use crate::state::State;
use itertools::Itertools;

fn get_cards_in_state(state: &State) -> Vec<Card> {
    let mut cards_in_state: Vec<Card> = Vec::new();
    cards_in_state.extend(state.deck.iter());
    cards_in_state.extend(state.pile.iter());
    for player in state.players.iter() {
        cards_in_state.extend(player.hand.iter());
        for trick in player.tricks.iter() {
            cards_in_state.extend(trick.iter());
        }
    }
    if let Some(trump) = state.trump {
        cards_in_state.push(trump);
    }
    cards_in_state
}

pub fn assert_invariants(state: &State) {
    let cards_in_state = get_cards_in_state(state);
    // Check that all cards in the state are unique
    assert_eq!(cards_in_state.len(), cards_in_state.iter().unique().count());
    // Check that there is the correct number of cards
    assert_eq!(cards_in_state.len(), Deck::new_full().len());
}

pub fn assert_player_tricks_empty(state: &State) {
    for player in state.players.iter() {
        assert_eq!(player.tricks.as_slice(), &[] as &[Vec<Card>]);
    }
}
pub fn assert_player_bets_empty(state: &State) {
    for player in state.players.iter() {
        assert_eq!(player.bet, None);
    }
}
pub fn assert_player_hands_empty(state: &State) {
    for player in state.players.iter() {
        assert_eq!(player.hand, vec![]);
    }
}
pub fn assert_pile_empty(state: &State) {
    assert_eq!(*state.pile, vec![]);
}
pub fn assert_cards_only_in_deck(state: &State) {
    let mut cards: Vec<Card> = Vec::new();
    cards.extend(state.deck.iter());
    if let Some(trump) = state.trump {
        cards.push(trump);
    }
    assert_eq!(get_cards_in_state(state).len(), cards.len());
    assert_eq!(cards.len(), Deck::new_full().len());
    assert_eq!(cards.iter().unique().count(), Deck::new_full().len());
}
pub fn assert_cards_only_in_deck_or_hands(state: &State) {
    let mut cards: Vec<Card> = Vec::new();
    cards.extend(state.deck.iter());
    if let Some(trump) = state.trump {
        cards.push(trump);
    }
    for player in state.players.iter() {
        cards.extend(player.hand.iter());
    }
    assert_eq!(cards.len(), Deck::new_full().len());
}
pub fn assert_all_players_have_same_number_of_cards(state: &State) {
    let hand_sizes = state
        .players
        .iter()
        .map(|player| player.hand.len())
        .collect::<Vec<usize>>();
    assert!(hand_sizes.iter().all(|&size| size == hand_sizes[0]));
}
pub fn assert_all_players_hand_empty(state: &State) {
    for player in state.players.iter() {
        assert_eq!(player.hand, vec![]);
    }
}
pub fn assert_all_players_bet_is_some(state: &State) {
    for player in state.players.iter() {
        assert!(player.bet.is_some());
    }
}
pub fn assert_all_players_bet_is_none(state: &State) {
    for player in state.players.iter() {
        assert!(player.bet.is_none());
    }
}
pub fn assert_active_player_is_none(state: &State) {
    assert_eq!(state.players.active_player_index, None);
}
pub fn assert_dealer_is_none(state: &State) {
    assert_eq!(state.players.dealer_index, None);
}
pub fn assert_active_player_is_some(state: &State) {
    assert!(state.players.active_player_index.is_some());
}
pub fn assert_dealer_is_some(state: &State) {
    assert!(state.players.dealer_index.is_some());
}
pub fn assert_trump_is_none(state: &State) {
    assert_eq!(state.trump, None);
}
pub fn assert_trump_is_some(state: &State) {
    assert!(state.trump.is_some());
}
pub fn assert_follow_suits_is_none(state: &State) {
    assert_eq!(state.get_suit_to_follow(), None);
}
pub fn assert_follow_suits_is_some(state: &State) {
    assert!(state.get_suit_to_follow().is_some());
}
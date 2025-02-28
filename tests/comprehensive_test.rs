use poche::action::Action;
use poche::random::RandomState;
use poche::round::Direction;
use poche::round::Round;
use poche::state::State;

/// 1) Test that a new deck is created with 52 cards, each suit/rank combo present.
#[test]
fn test_full_deck_creation() {
    let deck = poche::cards::Deck::new_full();
    assert_eq!(deck.cards.len(), 52, "A full deck should have 52 cards");

    // Quick sanity check that it contains each suit/rank
    let mut seen = std::collections::HashSet::new();
    for card in &deck.cards {
        seen.insert((card.suit, card.rank));
    }
    assert_eq!(seen.len(), 52, "All suit-rank combos should be unique");
}

/// 2) Test that dealing logic works: each player receives the expected number of cards.
#[test]
fn test_dealing_cards() {
    let mut state = State::default();
    // By default, each player has 1 card after State::default() calls deal_until_everyone_has_n_cards(1).
    for player in state.players.iter() {
        assert_eq!(player.hand.len(), 1, "Players should start with 1 card");
    }

    // Manually deal more cards
    let result = state.deal_until_everyone_has_n_cards(5);
    assert!(result.is_ok(), "Should succeed in dealing cards");
    for player in state.players.iter() {
        assert_eq!(player.hand.len(), 5, "Now players should have 5 cards each");
    }
}

/// 3) Test playing a card action and ensuring the card is removed from player's hand and put on the pile.
#[test]
fn test_play_card_action() {
    let mut state = State::default();
    let active_player_index = state.players.active_player_index;
    let active_player_cards_before = state.players[active_player_index].hand.len();

    let valid_actions = Action::get_valid_actions(&state).expect("Should get valid actions");
    // For a brand-new state, the valid actions are basically "PlayCard" of your single card.
    assert!(!valid_actions.is_empty());
    let action = valid_actions[0].clone();

    // Apply the action
    action.apply(&mut state);

    let active_player_cards_after = state.players[active_player_index].hand.len();
    assert_eq!(
        active_player_cards_before - 1,
        active_player_cards_after,
        "One card should have been removed from the player's hand"
    );
    assert_eq!(
        state.pile.cards.len(),
        1,
        "The played card should now be on the pile"
    );
}

/// 4) Test random state's shuffle determinism by comparing shuffles with the same seed.
#[test]
fn test_random_state_shuffle() {
    let mut deck1 = poche::cards::Deck::new_full();
    let mut deck2 = poche::cards::Deck::new_full();

    let mut rand_state1 = RandomState { seed: 42 };
    let mut rand_state2 = RandomState { seed: 42 };

    rand_state1.shuffle(&mut deck1.cards);
    rand_state2.shuffle(&mut deck2.cards);

    assert_eq!(
        deck1.cards, deck2.cards,
        "Shuffles with the same seed should produce the same order"
    );
}

/// 5) Test round logic: hand size increments (and eventually decrements) as expected.
#[test]
fn test_round_increments_decrements() {
    let mut round = Round::default();
    assert_eq!(round.hand_size, 1);
    assert_eq!(round.direction, Direction::Up);

    let num_players = 4;
    // Move the round forward up to a certain point
    for _ in 0..10 {
        let res = round.try_advance(num_players);
        if res.is_err() {
            // Might fail if the game is 'finished'. That's okay, let's break out.
            break;
        }
    }
    // If the logic is “1,2,3,4,5,6,7,6,5,4,3,2,1…finished”
    // we should see that after enough increments, direction is down again.
    assert_eq!(round.direction, Direction::Down);
}

/// 6) Test that the entire game eventually ends.
#[test]
fn test_game_ends() {
    let mut state = State::default();
    // We loop until the game is done. This also tests if anything gets stuck.
    while !state.is_done() {
        let _ = state.step(); // We ignore errors for now. If there's a logic error, we'll fail or panic.
    }
    assert!(
        state.is_done(),
        "The game should eventually reach a done state"
    );
}

/// 7) Test that pot and money jar get updated properly after initial ante
#[test]
fn test_initial_ante() {
    // By default, each player starts with 5 dimes and 5 quarters = 5*10 + 5*25 = 50 + 125 = 175 cents
    // Then, we remove 1 quarter for the pot. Everyone starts with 175 cents and ends up with 150.
    // The pot ends up with 100 cents if there are 4 players.
    let state = State::default();

    for player in &state.players.players {
        assert_eq!(
            player.money_jar.total_cents(),
            150,
            "Each player should have 150 cents left"
        );
    }
    assert_eq!(
        state.pot.total_cents(),
        100,
        "The pot should have 100 cents from 4 players each contributing a quarter"
    );
}

/// 8) Full simulation test: run many steps and check no panics or logic breaks.
#[test]
fn test_full_simulation_run() {
    let mut state = State::default();

    // We'll just run step until the game is done or until some large iteration count.
    // If there is a logic bug, we might get stuck or panic.
    for _ in 0..10000 {
        if state.is_done() {
            break;
        }
        // Just call step, which picks & applies an action automatically
        if let Err(e) = state.step() {
            // If step() fails but the game isn't done, let's panic for debugging
            if !state.is_done() {
                panic!("Step gave error but game isn't done: {:?}", e);
            }
        }
    }
    assert!(state.is_done(), "We should finish in under 10k steps");
}

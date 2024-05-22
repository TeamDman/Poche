use std::f32::consts::PI;
use std::time::Instant;

use bevy::app::AppExit;
/*
In the game Poche my family plays, the dealer is chosen by dealing a card to each player, high card deals.
If tied, deal to tied players, high card deals, repeat until no tie.
Each player pays a quarter into the pot at the start of the game.
Each round, the dealer deals cards clockwise, starting at their left, until each player has n cards where n is 1..7..1[round].
The top card of the deck is revealed, the suit of the card becomes trump. Trump cards are higher than non-trump cards.
The player to the left of the dealer starts the bidding.
Players bid how many tricks they think their hand will take.
The scorekeeper writes the bid below their name in a table.
Clockwise, every player places their bid.
Then, the player to the left of the dealer will lead the first hand.
The suit of the first card lead is the suit of the hand.
Players must play a card of the same suit if they have one.
The highest card takes the "trick"; the card pile is turned face down and placed distinctly from other piles in front of the player who took the trick.
The player who takes the trick leads the next hand.
After the last card is played, the scorekeeper updates each player's bid.
If you don't poche (you get the amount of tricks you said you would) your bid is prepended with a 1.
If you take all the tricks of the hand, a 2 is prepended instead.
If you poche, your bid is colored into a dot and you pay 10 cents to the pot; "ten cents a lesson".
The dealer rotates left.
Whoever has the most points at the end wins.
*/
use bevy::input::common_conditions::input_toggle_active;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::utils::HashMap;
use bevy::utils::HashSet;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_rts_camera::Ground;
use bevy_rts_camera::RtsCamera;
use bevy_rts_camera::RtsCameraControls;
use bevy_rts_camera::RtsCameraPlugin;
use itertools::Itertools;

////////////////////////////
/// APP
////////////////////////////

fn main() {
    let mut app = App::new();
    app.register_type::<Session>();
    app.register_type::<Card>();
    app.register_type::<InHand>();
    app.register_type::<InDeck>();
    app.register_type::<Trump>();
    app.register_type::<Played>();
    app.register_type::<CardPositioningBehaviour>();
    app.register_type::<Table>();
    app.register_type::<Player>();
    app.register_type::<SpawnTableEvent>();
    app.register_type::<BelongsToPlayer>();
    app.register_type::<Handles>();
    app.register_type::<Sleeping>();
    app.register_type::<TablePositions>();

    app.init_resource::<Handles>();
    app.init_resource::<TablePositions>();

    app.add_event::<SpawnTableEvent>();
    app.add_event::<SpawnDeckEvent>();
    app.add_event::<DealCardsEvent>();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    // cursor: bevy::window::Cursor {
                    //     grab_mode: bevy::window::CursorGrabMode::Confined,
                    //     ..default()
                    // },
                    ..default()
                }),
                ..default()
            })
            .set(LogPlugin {
                level: bevy::log::Level::INFO,
                filter: "
                    wgpu=error
                    poche=debug
                "
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.starts_with("//"))
                .filter(|line| !line.is_empty())
                .join(",")
                .trim()
                .into(),
                ..default()
            }),
    );
    app.add_plugins(
        WorldInspectorPlugin::default().run_if(input_toggle_active(false, KeyCode::Backquote)),
    );
    app.add_plugins(RtsCameraPlugin);

    app.add_systems(Startup, setup);
    app.add_systems(
        Update,
        (
            handle_spawn_table_events,
            handle_spawn_deck_events,
            handle_tables_needing_dealer,
            handle_deal_cards_events,
            determine_card_positioning_behaviours,
            handle_cards_positioning_cards_in_deck,
            handle_cards_positioning_cards_in_hand,
        )
            .chain(),
    );
    app.add_systems(Update, handle_quit_key_press);
    app.add_systems(Update, handle_deal_key_press);
    app.add_systems(Update, handle_sleeping_key_press);
    app.add_systems(Update, handle_shuffle_back_in_key_press);
    app.add_systems(Update, update_card_names);

    app.run();
}

////////////////////////////
/// CARDS
////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, Copy, Reflect, Hash)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}
#[derive(Debug, Eq, PartialEq, Clone, Copy, Reflect, Hash)]
pub enum Rank {
    Ace,
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
}
impl Rank {
    pub fn value(&self) -> u8 {
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

#[derive(Component, Debug, Eq, PartialEq, Clone, Copy, Reflect, Hash)]
pub struct Card {
    suit: Suit,
    rank: Rank,
}
impl Card {
    pub fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }
    pub fn get_texture_path(&self) -> String {
        let suit = match self.suit {
            Suit::Spades => "Spades",
            Suit::Hearts => "Hearts",
            Suit::Diamonds => "Diamonds",
            Suit::Clubs => "Clubs",
        };
        let rank = match self.rank {
            Rank::Ace => "A",
            Rank::Two => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "Jack",
            Rank::Queen => "Queen",
            Rank::King => "King",
        };
        format!("cards/{suit}_{rank}_white.png")
    }

    pub fn get_new_deck() -> Vec<Self> {
        let mut cards = Vec::new();
        for &suit in &[Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs] {
            for &rank in &[
                Rank::Ace,
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
            ] {
                cards.push(Card::new(suit, rank));
            }
        }
        cards
    }
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct InHand {
    index_from_left: usize,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct Played;

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct InDeck {
    index_from_bottom: usize,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct Trump;

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct BelongsToPlayer(Entity);

/// To avoid conflicting card transform updates, use a component to enforce exclusive update bahviour.
#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub enum CardPositioningBehaviour {
    InDeck,
    RevealedOnDeck,
    InHand,
    Played,
    InTakenTrick,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct TravelTime {
    start_time: Instant,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct Sleeping {
    start_time: Instant,
}

////////////////////////////
/// HANDLES
////////////////////////////
#[derive(Resource, Debug, Reflect, Default)]
pub struct Handles {
    pub table_shape: Cylinder,
    pub table_mesh: Handle<Mesh>,
    pub table_material: Handle<StandardMaterial>,
    pub card_shape: Cuboid,
    pub card_mesh: Handle<Mesh>,
    pub card_materials: HashMap<Card, Handle<StandardMaterial>>,
    pub player_body_shape: Capsule3d,
    pub player_body_mesh: Handle<Mesh>,
    pub player_body_material: Handle<StandardMaterial>,
    pub player_eye_shape: Sphere,
    pub player_eye_mesh: Handle<Mesh>,
    pub player_eye_material: Handle<StandardMaterial>,
}

////////////////////////////
/// TABLE
////////////////////////////

#[derive(Resource, Debug, Reflect, Default)]
pub struct TablePositions {
    pub positions: Vec<Vec3>,
}
impl TablePositions {
    /// Get a position that is separated from all other tables.
    ///
    /// Places tables in a spiral.
    ///
    /// Fills in gaps from released tables.
    pub fn acquire_position(&mut self, y: f32) -> Vec3 {
        let mut angle: f32 = 0.0;
        let mut radius = 0.0;
        let mut position: Vec3;
        let spread = 8.0;
        loop {
            position = Vec3::new(radius * angle.cos(), y, radius * angle.sin());
            if self
                .positions
                .iter()
                .all(|&p| (p - position).length() > spread)
            {
                break;
            }
            angle += 0.1;
            if angle > std::f32::consts::PI * 2.0 {
                angle = 0.0;
                radius += spread;
            }
        }
        self.positions.push(position);
        position
    }
    pub fn release_position(&mut self, position: Vec3) {
        self.positions.retain(|&p| p != position);
    }
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct Table;

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct NeedsDealer;

////////////////////////////
/// SESSION
////////////////////////////

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct Session {
    table_id: Entity,
    player_ids: HashSet<Entity>,
    card_ids: HashSet<Entity>,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct SessionRef(Entity);
impl std::ops::Deref for SessionRef {
    type Target = Entity;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SessionRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

////////////////////////////
/// PLAYERS
////////////////////////////

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct Player;

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct Dealer;

////////////////////////////
/// EVENTS
////////////////////////////

/// You can deal to the same player multiple times.
#[derive(Event, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct DealCardsEvent {
    pub session_id: Entity,
    pub player_ids: Vec<Entity>,
}

#[derive(Event, Debug, Reflect)]
pub struct SpawnTableEvent {
    pub num_players: usize,
}
#[derive(Event, Debug, Reflect)]
pub struct SpawnDeckEvent {
    pub session_id: Entity,
}

////////////////////////////
/// SYSTEMS
////////////////////////////
fn handle_quit_key_press(mut exit: ResMut<Events<AppExit>>, input: Res<ButtonInput<KeyCode>>) {
    if input.just_pressed(KeyCode::Escape) {
        exit.send(AppExit);
    }
}

fn handle_deal_key_press(
    mut events: EventWriter<DealCardsEvent>,
    input: Res<ButtonInput<KeyCode>>,
    session_query: Query<(Entity, &Session)>,
) {
    if input.just_pressed(KeyCode::Digit1) {
        for session in session_query.iter() {
            let (session_id, session) = session;
            let player_ids = session.player_ids.iter().cloned().collect_vec();
            info!("Dealing cards to all players in session {session_id:?} because of key press");
            events.send(DealCardsEvent {
                session_id,
                player_ids,
            });
        }
    }
}

fn handle_spawn_table_events(
    mut commands: Commands,
    mut spawn_table_events: EventReader<SpawnTableEvent>,
    mut spawn_deck_events: EventWriter<SpawnDeckEvent>,
    mut table_positions: ResMut<TablePositions>,
    handles: Res<Handles>,
) {
    for event in spawn_table_events.read() {
        // Reserve the table position
        let table_position = table_positions.acquire_position(handles.table_shape.half_height);

        // Spawn the players in a circle around the table
        let mut players = Vec::new();
        let seating_radius = handles.table_shape.radius + 0.7;
        for i in 0..event.num_players {
            // Get the angle from the center of the table to the player
            let player_angle = Quat::from_rotation_y(
                std::f32::consts::PI * 2.0 * i as f32 / event.num_players as f32,
            );

            // Get the position of the player
            let player_position = table_position
                + player_angle.mul_vec3(Vec3::X * seating_radius)
                + Vec3::Y
                    * (handles.table_shape.half_height
                        + handles.player_body_shape.half_length / 2.0);

            // Get angle so player faces center of table
            let player_rotation = Quat::from_rotation_y(
                (player_position - table_position)
                    .x
                    .atan2((player_position - table_position).z),
            );

            let player = commands
                .spawn((
                    PbrBundle {
                        mesh: handles.player_body_mesh.clone(),
                        material: handles.player_body_material.clone(),
                        transform: Transform::from_rotation(player_rotation)
                            .with_translation(player_position),
                        ..default()
                    },
                    Player::default(),
                    Name::new("Player"),
                ))
                .with_children(|parent| {
                    // spawn the eyes
                    parent.spawn(PbrBundle {
                        mesh: handles.player_eye_mesh.clone(),
                        material: handles.player_eye_material.clone(),
                        transform: Transform::from_xyz(
                            0.1,
                            handles.player_body_shape.half_length * 0.8,
                            -handles.player_body_shape.radius,
                        ),
                        ..default()
                    });
                    parent.spawn(PbrBundle {
                        mesh: handles.player_eye_mesh.clone(),
                        material: handles.player_eye_material.clone(),
                        transform: Transform::from_xyz(
                            -0.1,
                            handles.player_body_shape.half_length * 0.8,
                            -handles.player_body_shape.radius,
                        ),
                        ..default()
                    });
                })
                .id();
            players.push((player, player_position));
        }

        // Spawn the table
        let table = commands
            .spawn((
                PbrBundle {
                    mesh: handles.table_mesh.clone(),
                    material: handles.table_material.clone(),
                    transform: Transform::from_translation(table_position),
                    ..default()
                },
                Name::new("Table"),
                Table,
                NeedsDealer,
            ))
            .id();

        // Create the session
        let session_id = commands
            .spawn((
                Session {
                    table_id: table,
                    player_ids: players.iter().map(|(p, _)| *p).collect(),
                    card_ids: Default::default(),
                },
                Name::new("Session"),
            ))
            .id();

        // Attach session
        for player in players {
            commands.entity(player.0).insert(SessionRef(session_id));
        }
        commands.entity(table).insert(SessionRef(session_id));

        // Spawn the deck
        spawn_deck_events.send(SpawnDeckEvent { session_id });

        // Spawn the light
        commands.spawn(PointLightBundle {
            transform: Transform::from_translation(table_position + Vec3::Y * 4.0),
            ..default()
        });

        info!("Table spawned with {} players", event.num_players);
    }
}

fn handle_spawn_deck_events(
    mut commands: Commands,
    mut spawn_deck_events: EventReader<SpawnDeckEvent>,
    mut session_query: Query<&mut Session>,
    table_query: Query<&Transform, With<Table>>,
    handles: Res<Handles>,
) {
    for event in spawn_deck_events.read() {
        let Ok(mut session) = session_query.get_mut(event.session_id) else {
            warn!("Session not found for deck spawn event");
            continue;
        };

        // Get the table transform
        let Ok(table) = table_query.get(session.table_id) else {
            warn!("Table not found for deck spawn event");
            continue;
        };
        let table_transform = table;

        // Calculate deck position
        let deck_position = table_transform.translation
            + Vec3::Y * handles.table_shape.half_height
            + Vec3::Y * handles.card_shape.half_size.y;

        // Spawn the deck by spawning in each card
        let y_increment = 0.01;
        let mut y = 0.0;
        let cards = Card::get_new_deck();
        for (i, card) in cards.into_iter().enumerate() {
            let card_position = deck_position + Vec3::Y * y;
            let card_id = commands
                .spawn((
                    PbrBundle {
                        mesh: handles.card_mesh.clone(),
                        material: handles.card_materials.get(&card).unwrap().clone(),
                        transform: Transform::from_translation(card_position),
                        ..default()
                    },
                    card,
                    Name::new("Card"),
                    InDeck {
                        index_from_bottom: i,
                    },
                ))
                .id();
            session.card_ids.insert(card_id);

            y += y_increment;
        }

        info!("Deck spawned");
    }
}

/// Determine state through observation.
///
/// If no cards in hands, or uneven amounts, dispatch deal card event.
///
/// If all players
fn handle_tables_needing_dealer(
    mut commands: Commands,
    table_query: Query<(Entity, &SessionRef), (With<NeedsDealer>, With<Table>)>,
    session_query: Query<&Session>,
    cards_in_hands_query: Query<(&Card, Option<&BelongsToPlayer>), With<InHand>>,
    mut deal_card_events: EventWriter<DealCardsEvent>,
) {
    for table in table_query.iter() {
        let (table_id, table_session_id) = table;
        // Get the session
        let Ok(session) = session_query.get(**table_session_id) else {
            warn!("Session {table_session_id:?} not found for table {table_id:?} needing dealer");
            continue;
        };

        if session.player_ids.len() < 2 {
            warn!("Table has less than 2 players, not starting dealer selection");
            continue;
        }

        // Identify the cards in each hand
        let cards_by_player =
            session
                .card_ids
                .iter()
                .fold(HashMap::default(), |mut map, card_id| {
                    // Get card
                    let Ok((card, player_id)) = cards_in_hands_query.get(*card_id) else {
                        return map;
                    };
                    // Add to player's hand
                    if let Some(player_id) = player_id {
                        map.entry(player_id.0)
                            .or_insert_with(Vec::new)
                            .push((card_id, card));
                    }

                    map
                });

        {
            // Find players with no cards in hand
            let no_cards_in_hand = session
                .player_ids
                .iter()
                .filter(|player_id| {
                    cards_by_player
                        .get(*player_id)
                        .map_or(true, |cards| cards.is_empty())
                })
                .cloned()
                .collect_vec();

            // If any, deal and continue
            if !no_cards_in_hand.is_empty() {
                // Deal cards to those players
                info!("Dealing cards to players with no cards in hand");
                deal_card_events.send(DealCardsEvent {
                    session_id: **table_session_id,
                    player_ids: no_cards_in_hand,
                });
                continue;
            }
        }

        // All players have at least one card
        {
            // compute the value of each hand
            let player_values = cards_by_player
                .iter()
                .map(|(player_id, cards)| {
                    let value = cards
                        .iter()
                        .map(|(_, card)| card.rank.value() as usize)
                        .sum();
                    (*player_id, value)
                })
                .collect::<HashMap<Entity, usize>>();

            // find max value
            let max_value = player_values
                .values()
                .max()
                .copied()
                .expect("there should be at least one player with a card by now");

            // find tied for first
            let players_with_max_value = player_values
                .iter()
                .filter_map(|(player_id, value)| {
                    if *value == max_value {
                        Some(*player_id)
                    } else {
                        None
                    }
                })
                .collect::<Vec<Entity>>();
            assert!(!players_with_max_value.is_empty());

            // solo winner becomes dealer
            if players_with_max_value.len() == 1 {
                commands.entity(session.table_id).remove::<NeedsDealer>();
                commands.entity(players_with_max_value[0]).insert(Dealer);
                info!("Dealer selected");
                continue;
            }

            // tied winners must draw again
            info!("Tie for dealer, dealing again");
            deal_card_events.send(DealCardsEvent {
                session_id: **table_session_id,
                player_ids: players_with_max_value,
            });
        }
    }
}

fn handle_deal_cards_events(
    mut commands: Commands,
    cards_in_decks_query: Query<&Transform, With<InDeck>>,
    cards_in_hands_query: Query<(&BelongsToPlayer, &InHand), With<Card>>,
    mut deal_cards_events: EventReader<DealCardsEvent>,
    session_query: Query<&Session>,
) {
    if deal_cards_events.is_empty() {
        return;
    }

    // Get the size of hands for each player so we know what index to start at
    let mut hand_sizes =
        cards_in_hands_query
            .iter()
            .fold(HashMap::default(), |mut map, (player, _)| {
                *map.entry(player.0).or_insert(0) += 1;
                map
            });

    for event in deal_cards_events.read() {
        let players = &event.player_ids;

        // Get the session
        let session_id = event.session_id;
        let Ok(session) = session_query.get(session_id) else {
            warn!("Session {session_id:?} not found for deal cards event");
            continue;
        };

        // Get cards from the top of the deck
        let top_cards = session
            .card_ids
            .iter()
            .filter_map(|card_id| {
                let Ok(card_transform) = cards_in_decks_query.get(*card_id) else {
                    return None;
                };
                Some((card_id, card_transform.translation.y))
            })
            .sorted_by_key(|(_, y)| (1000.0 * y) as i32)
            .rev()
            .map(|(card_id, _)| *card_id);

        // Check if there are enough cards to deal
        if players.len() > top_cards.len() {
            warn!("A deal was started with insufficient amount of cards!");
            debug!("Players: {:?}", players);
            debug!("Top cards: {:?}", top_cards);
        }

        // Update state
        players
            .iter()
            .zip(top_cards)
            .for_each(|(player_id, card_id)| {
                // Take it out of the deck
                commands.entity(card_id).remove::<InDeck>();

                // Make it belong to the player
                commands.entity(card_id).insert(BelongsToPlayer(*player_id));

                // Put it in the player's hand
                let hand_size = hand_sizes.get(player_id).unwrap_or(&0);
                commands.entity(card_id).insert(InHand {
                    index_from_left: *hand_size,
                });
                hand_sizes.get_mut(player_id).map(|size| *size += 1);

                // Wake it up
                commands.entity(card_id).remove::<Sleeping>();

                info!("Dealt card {:?} to player {:?}", card_id, player_id);
            });

        info!("Dealt cards to players");
    }
}

fn determine_card_positioning_behaviours(
    mut commands: Commands,
    card_query: Query<
        (
            Entity,
            Option<&CardPositioningBehaviour>,
            Option<&BelongsToPlayer>,
            Option<&InDeck>,
            Option<&InHand>,
            Option<&Played>,
            Option<&Trump>,
        ),
        With<Card>,
    >,
) {
    for card in card_query.iter() {
        let (
            card_id,
            card_positioning_behaviour,
            card_player,
            card_in_deck,
            card_in_hand,
            card_played,
            card_trump,
        ) = card;

        struct Decision {
            has_player: bool,
            in_deck: bool,
            in_hand: bool,
            played: bool,
            trump: bool,
        }
        let decision = Decision {
            has_player: card_player.is_some(),
            in_deck: card_in_deck.is_some(),
            in_hand: card_in_hand.is_some(),
            played: card_played.is_some(),
            trump: card_trump.is_some(),
        };
        match match decision {
            Decision {
                in_hand: true,
                has_player,
                ..
            } => {
                if !has_player {
                    warn!("Card in hand without player");
                }
                Some(CardPositioningBehaviour::InHand)
            }
            Decision { in_deck: true, .. } => Some(CardPositioningBehaviour::InDeck),
            Decision { played: true, .. } => Some(CardPositioningBehaviour::Played),
            Decision { trump: true, .. } => Some(CardPositioningBehaviour::RevealedOnDeck),
            _ => None,
        } {
            Some(behaviour) => {
                if card_positioning_behaviour != Some(&behaviour) {
                    debug!(
                        "Card {:?} changed now using positioning behaviour: {:?}",
                        card_id, behaviour
                    );
                }
                commands.entity(card_id).insert(behaviour);
            }
            None => {
                if card_positioning_behaviour.is_some() {
                    warn!(
                        "Card {:?} changed now using positioning behaviour: None",
                        card_id
                    );
                }
                commands
                    .entity(card_id)
                    .remove::<CardPositioningBehaviour>();
            }
        }
    }
}

// todo: add a "Sleeping" component to avoid movement calculations on entities at rest

fn handle_cards_positioning_cards_in_deck(
    mut commands: Commands,
    session_query: Query<&Session>,
    mut cards_in_decks_query: Query<
        (
            &CardPositioningBehaviour,
            &mut Transform,
            &InDeck,
            Option<&TravelTime>,
            Option<&Sleeping>,
        ),
        With<Card>,
    >,
    table_query: Query<&Transform, (With<Table>, Without<Card>)>,
    handles: Res<Handles>,
) {
    for session in session_query.iter() {
        let mut cards_in_deck = session
            .card_ids
            .iter()
            .filter_map(|card_id| {
                let Ok(card) = cards_in_decks_query.get(*card_id) else {
                    return None;
                };
                Some((card_id, card))
            })
            .sorted_by_key(|(_card_id, (_, _, in_deck, ..))| in_deck.index_from_bottom)
            .peekable();
        let bottom_card_id = match cards_in_deck.peek() {
            Some((card_id, _)) => *card_id,
            None => {
                // no cards in deck
                continue;
            }
        };

        let cards_to_position = cards_in_deck
            .filter(
                |(_card_id, (behaviour, _transform, _in_deck, _travel_time, sleeping))| {
                    sleeping.is_none() && matches!(behaviour, CardPositioningBehaviour::InDeck)
                },
            )
            .map(|(card_id, _)| card_id)
            .cloned()
            .collect_vec();

        let Ok((_, bottom_card_transform, ..)) = cards_in_decks_query.get(*bottom_card_id) else {
            // bottom card not found
            continue;
        };
        let bottom_card_transform = bottom_card_transform.to_owned();

        for card_id in cards_to_position {
            let Ok(mut card) = cards_in_decks_query.get_mut(card_id) else {
                warn!("Card not found in deck");
                continue;
            };
            // get values
            let i = card.2.index_from_bottom;
            let card_transform = &mut *card.1;
            let travel_time = card.3;

            // calculate positions
            let (desired_pos, desired_rot) = match i {
                0 => {
                    // bottom card is at table surface
                    let Ok(table) = table_query.get(session.table_id) else {
                        warn!("Table not found for deck positioning");
                        continue;
                    };
                    let table_transform = table;
                    (
                        table_transform.translation
                            + Vec3::Y
                                * (handles.table_shape.half_height
                                    + handles.card_shape.half_size.y),
                        Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0),
                    )
                }
                _ => {
                    // other cards are stacked on top
                    let desired_pos = bottom_card_transform.translation + Vec3::Y * i as f32 * 0.01;
                    let desired_rot = bottom_card_transform.rotation;
                    (desired_pos, desired_rot)
                }
            };

            let current_pos = card_transform.translation;
            let current_rot = card_transform.rotation;

            // get or set travel start time
            let travel_time = match travel_time {
                Some(travel_time) => travel_time.start_time.to_owned(),
                None => {
                    let now = Instant::now();
                    commands
                        .entity(card_id)
                        .insert(TravelTime { start_time: now });
                    now
                }
            };

            // calculate progress
            let progress = travel_time.elapsed().as_secs_f32();
            let progress = progress.min(1.0);
            let progress = progress.powf(0.5);

            // update card position
            card_transform.translation = current_pos.lerp(desired_pos, progress);
            card_transform.rotation = current_rot.slerp(desired_rot, progress);

            if progress >= 0.99 {
                commands.entity(card_id).remove::<TravelTime>();
                commands.entity(card_id).insert(Sleeping {
                    start_time: Instant::now(),
                });
            }
        }
    }
}

fn handle_cards_positioning_cards_in_hand(
    mut commands: Commands,
    session_query: Query<&Session>,
    mut cards_in_hands_query: Query<
        (
            &CardPositioningBehaviour,
            &mut Transform,
            &InHand,
            &BelongsToPlayer,
            Option<&TravelTime>,
            Option<&Sleeping>,
        ),
        (With<Card>, Without<Player>),
    >,
    player_query: Query<&Transform, (With<Player>, Without<Card>)>,
) {
    for session in session_query.iter() {
        for player_id in session.player_ids.iter() {
            let Ok(player) = player_query.get(*player_id) else {
                warn!("Player not found in hand");
                continue;
            };
            let player_transform = player;

            // Identify the cards in the player's hand
            let mut cards_in_hand = session
                .card_ids
                .iter()
                .filter_map(|card_id| {
                    // Get card
                    let Ok(card) = cards_in_hands_query.get(*card_id) else {
                        return None;
                    };
                    let (
                        _card_positioning_behaviour,
                        _card_transform,
                        in_hand,
                        belongs_to_player,
                        _travel_start_time,
                        sleeping,
                    ) = card;

                    // Check if the card belongs to the player
                    if belongs_to_player.0 != *player_id {
                        return None;
                    }

                    Some((card_id, _card_positioning_behaviour, in_hand, sleeping))
                })
                .sorted_by_key(
                    |(_card_id, _card_positioning_behaviour, in_hand, _sleeping)| {
                        in_hand.index_from_left
                    },
                )
                .map(|(card_id, card_positioning_behaviour, _, sleeping, ..)| {
                    (card_id, card_positioning_behaviour, sleeping)
                })
                .peekable();

            // The first card in hand is the leftmost card
            let Some((left_card_id, ..)) = cards_in_hand.peek() else {
                // no cards in hand
                continue;
            };
            let Ok((_, left_card_transform, ..)) = cards_in_hands_query.get(**left_card_id) else {
                // left card not found
                continue;
            };
            let left_card_transform = left_card_transform.to_owned();

            // Get the cards set to this behaviour
            let cards_to_position = cards_in_hand
                .filter(|(_card_id, card_positioning_behaviour, sleeping)| {
                    sleeping.is_none()
                        && matches!(card_positioning_behaviour, CardPositioningBehaviour::InHand)
                })
                .map(|(card_id, ..)| card_id)
                .cloned()
                .collect_vec();

            // debug!(
            //     "Player {:?} has {} cards in hand",
            //     player_id,
            //     cards_to_position.len()
            // );

            for card_id in cards_to_position {
                let Ok(mut card) = cards_in_hands_query.get_mut(card_id) else {
                    warn!("Card not found in hand");
                    continue;
                };
                // get values
                let i = card.2.index_from_left;
                let card_transform = &mut *card.1;
                let travel_time = card.4;

                // calculate positions
                let (desired_pos, desired_rot) = match i {
                    0 => {
                        // the leftmost card starts in front of the player
                        (
                            player_transform.translation + player_transform.forward() * 0.5,
                            player_transform.rotation
                                * Quat::from_euler(EulerRot::XYZ, PI / 2.0, 0.0, 0.0),
                        )
                    }
                    _ => {
                        let desired_pos = left_card_transform.translation
                            + left_card_transform.right() * i as f32 * 0.1;
                        let desired_rot = left_card_transform.rotation;
                        (desired_pos, desired_rot)
                    }
                };

                let current_pos = card_transform.translation;
                let current_rot = card_transform.rotation;

                // get or set travel start time
                let travel_start_time = match travel_time {
                    Some(travel_time) => travel_time.start_time.to_owned(),
                    None => {
                        let now = Instant::now();
                        commands
                            .entity(card_id)
                            .insert(TravelTime { start_time: now });
                        now
                    }
                };

                // calculate progress
                let progress = travel_start_time.elapsed().as_secs_f32();
                let progress = progress.min(1.0);
                let progress = progress.powf(0.5);

                // update card position
                card_transform.translation = current_pos.lerp(desired_pos, progress);
                card_transform.rotation = current_rot.slerp(desired_rot, progress);

                if progress >= 0.99 {
                    commands.entity(card_id).remove::<TravelTime>();
                    commands.entity(card_id).insert(Sleeping {
                        start_time: Instant::now(),
                    });
                }
            }
        }
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut reset_events: EventWriter<SpawnTableEvent>,
    mut handles: ResMut<Handles>,
    asset_server: Res<AssetServer>,
) {
    // table
    handles.table_shape = Cylinder::new(2.0, 1.0);
    handles.table_mesh = meshes.add(handles.table_shape.clone());
    handles.table_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    // deck
    let card_tex_width = 655.0;
    let card_tex_height = 930.0;
    let card_aspect = card_tex_width / card_tex_height;
    // let card_width = 0.25;
    let card_width = 0.3;
    let card_height = card_width * card_aspect;
    handles.card_shape = Cuboid::new(card_height, 0.005, card_width);
    handles.card_mesh = meshes.add(handles.card_shape.clone());
    handles.card_materials = Card::get_new_deck()
        .into_iter()
        .map(|card| {
            let texture_handle = asset_server.load(card.get_texture_path());
            (
                card,
                materials.add(StandardMaterial {
                    base_color_texture: Some(texture_handle),
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                }),
            )
        })
        .collect();

    // player
    handles.player_body_shape = Capsule3d::new(0.2, 0.5);
    handles.player_body_mesh = meshes.add(handles.player_body_shape.clone());
    handles.player_body_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.0, 0.0, 1.0),
        ..default()
    });
    handles.player_eye_shape = Sphere::new(0.05);
    handles.player_eye_mesh = meshes.add(handles.player_eye_shape.clone());
    handles.player_eye_material = materials.add(StandardMaterial {
        base_color: Color::rgb(1.0, 1.0, 1.0),
        ..default()
    });

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(5.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        RtsCamera::default(),
        RtsCameraControls {
            // https://github.com/Plonq/bevy_rts_camera/blob/main/examples/advanced.rs
            // Change pan controls to WASD
            key_up: KeyCode::KeyW,
            key_down: KeyCode::KeyS,
            key_left: KeyCode::KeyA,
            key_right: KeyCode::KeyD,
            // Rotate the camera with right click
            button_rotate: MouseButton::Right,
            // Keep the mouse cursor in place when rotating
            lock_on_rotate: true,
            // Drag pan with middle click
            button_drag: Some(MouseButton::Middle),
            // Keep the mouse cursor in place when dragging
            lock_on_drag: true,
            // Change the width of the area that triggers edge pan. 0.1 is 10% of the window height.
            // edge_pan_width: 0.1,
            // Increase pan speed
            pan_speed: 25.0,
            ..default()
        },
    ));

    // ground
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Plane3d::default().mesh().size(80.0, 80.0)),
            material: materials.add(StandardMaterial {
                base_color: Color::rgba(0.0, 0.5, 0.0, 0.1),
                perceptual_roughness: 0.9,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            ..default()
        },
        Ground,
        Name::new("Ground"),
    ));

    // spawn the first table
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
}

fn handle_sleeping_key_press(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    sleeping_query: Query<Entity, With<Sleeping>>,
) {
    if input.just_pressed(KeyCode::KeyF) {
        let mut n = 0;
        for sleeping in sleeping_query.iter() {
            commands.entity(sleeping).remove::<Sleeping>();
            n += 1;
        }
        info!("Woke up {} entities", n);
    }
}

fn handle_shuffle_back_in_key_press(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    session_query: Query<&Session>,
) {
    if input.just_pressed(KeyCode::KeyR) {
        // Move all cards to the deck
        for session in session_query.iter() {
            let mut count = 0;
            for (i, card_id) in session.card_ids.iter().enumerate() {
                // Remove from play
                commands.entity(*card_id).remove::<InHand>();
                commands.entity(*card_id).remove::<Played>();
                commands.entity(*card_id).remove::<Trump>();
                commands.entity(*card_id).remove::<BelongsToPlayer>();

                // Add to deck
                commands.entity(*card_id).insert(InDeck {
                    index_from_bottom: i,
                });

                // Wake up
                commands.entity(*card_id).remove::<Sleeping>();

                count += 1;
            }
            info!("Moved {} cards back into the deck", count);
        }
    }
}

fn update_card_names(
    mut card_query: Query<(&mut Name, Option<&InHand>, Option<&InDeck>), With<Card>>,
) {
    for card in card_query.iter_mut() {
        let (mut name, in_hand, in_deck) = card;
        *name = Name::new(format!("Card ({in_hand:?},{in_deck:?})"));
    }
}

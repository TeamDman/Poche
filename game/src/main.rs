/*
In the game Poche my family plays, the dealer is chosen by dealing a card to each player, high card deals.
If tied, deal to tied player, high card deals, repeat until no tie.
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
The highest card takes the "trick".
The player who takes the trick leads the next hand.
After the last card is played, the scorekeeper updates each player's bid.
If you don't poche (you get the amount of tricks you said you would) your bid is prepended with a 1.
If you take all the tricks of the hand, a 2 is prepended instead.
If you poche, your bid is colored into a dot and you pay 10 cents to the pot; "ten cents a lesson".
The dealer rotates left.
Whoever has the most points at the end wins. 
*/

use bevy::{input::common_conditions::input_toggle_active, prelude::*, window::Cursor};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_rts_camera::{Ground, RtsCamera, RtsCameraControls, RtsCameraPlugin};

////////////////////////////
/// APP
////////////////////////////

fn main() {
    let mut app = App::new();
    app.register_type::<Card>();
    app.register_type::<Deck>();
    app.register_type::<Table>();
    app.register_type::<Player>();
    app.register_type::<SpawnTableEvent>();
    app.register_type::<Handles>();
    app.register_type::<TablePositions>();

    app.init_resource::<Handles>();
    app.init_resource::<TablePositions>();

    app.add_event::<SpawnTableEvent>();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            cursor: Cursor {
                grab_mode: bevy::window::CursorGrabMode::Confined,
                ..default()
            },
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(
        WorldInspectorPlugin::default().run_if(input_toggle_active(false, KeyCode::Backquote)),
    );
    app.add_plugins(RtsCameraPlugin);

    app.add_systems(Startup, setup);
    app.add_systems(Update, handle_spawn_table_events);

    app.run();
}

////////////////////////////
/// TYPES
////////////////////////////

#[derive(Debug, Eq, PartialEq, Clone, Copy, Reflect)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}
#[derive(Debug, Eq, PartialEq, Clone, Copy, Reflect)]
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

#[derive(Component, Debug, Eq, PartialEq, Clone, Copy, Reflect)]
pub struct Card {
    suit: Suit,
    rank: Rank,
}
impl Card {
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
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect)]
pub struct Deck {
    cards: Vec<Card>,
}
impl Default for Deck {
    fn default() -> Self {
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
                cards.push(Card { suit, rank });
            }
        }
        Self { cards }
    }
}

#[derive(Resource, Debug, Reflect, Default)]
pub struct Handles {
    pub table_shape: Cylinder,
    pub table_mesh: Handle<Mesh>,
    pub table_material: Handle<StandardMaterial>,
    pub deck_shape: Cuboid,
    pub deck_mesh: Handle<Mesh>,
    pub deck_material: Handle<StandardMaterial>,
    pub player_shape: Capsule3d,
    pub player_mesh: Handle<Mesh>,
    pub player_material: Handle<StandardMaterial>,
}

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
    pub fn acquire_position(&mut self) -> Vec3 {
        let mut angle: f32 = 0.0;
        let mut radius = 0.0;
        let mut position: Vec3;
        let spread = 8.0;
        loop {
            position = Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());
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
pub struct Table {
    players: Vec<Entity>,
    deck: Entity,
    dealer: Entity,
}

#[derive(Component, Debug, Eq, PartialEq, Clone, Reflect, Default)]
pub struct Player {
    cards_in_hand: Vec<Entity>,
}

#[derive(Event, Debug, Reflect)]
pub struct SpawnTableEvent {
    pub num_players: usize,
}

////////////////////////////
/// SYSTEMS
////////////////////////////

fn handle_spawn_table_events(
    mut commands: Commands,
    mut spawn_events: EventReader<SpawnTableEvent>,
    mut table_positions: ResMut<TablePositions>,
    handles: Res<Handles>,
) {
    for event in spawn_events.read() {
        // Reserve the table position
        let table_position = table_positions.acquire_position();

        // Spawn the players in a circle around the table
        let mut players = Vec::new();
        let seating_radius = handles.table_shape.radius + 0.7;
        for i in 0..event.num_players {
            let player_position = table_position
                + Vec3::new(
                    seating_radius
                        * (std::f32::consts::TAU * i as f32 / event.num_players as f32).cos(),
                    handles.table_shape.half_height + handles.player_shape.half_length,
                    seating_radius
                        * (std::f32::consts::TAU * i as f32 / event.num_players as f32).sin(),
                );
            let player = commands
                .spawn((
                    PbrBundle {
                        mesh: handles.player_mesh.clone(),
                        material: handles.player_material.clone(),
                        transform: Transform::from_translation(player_position),
                        ..default()
                    },
                    Player::default(),
                    Name::new("Player"),
                ))
                .id();
            players.push((player, player_position));
        }

        // Choose a dealer (for simplicity, choosing the first player as the dealer)
        let dealer = players[0];

        // Spawn the deck and position it relative to the dealer
        let deck_position = dealer.1
            + Vec3::new(
                seating_radius * 0.3 * (std::f32::consts::PI / 2.0).cos(),
                0.0,
                seating_radius * 0.3 * (std::f32::consts::PI / 2.0).sin(),
            );

        // Spawn the deck
        let deck = commands
            .spawn((
                PbrBundle {
                    mesh: handles.deck_mesh.clone(),
                    material: handles.deck_material.clone(),
                    transform: Transform::from_translation(deck_position),
                    ..default()
                },
                Deck::default(),
                Name::new("Deck"),
            ))
            .id();

        // Spawn the table
        let _table = commands
            .spawn((
                PbrBundle {
                    mesh: handles.table_mesh.clone(),
                    material: handles.table_material.clone(),
                    transform: Transform::from_translation(table_position),
                    ..default()
                },
                Name::new("Table"),
                Table {
                    deck,
                    players: players.into_iter().map(|x| x.0).collect(),
                    dealer: dealer.0,
                },
            ))
            .id();

        // light
        commands.spawn(PointLightBundle {
            transform: Transform::from_translation(table_position + Vec3::Y * 4.0),
            ..default()
        });
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut reset_events: EventWriter<SpawnTableEvent>,
    mut handles: ResMut<Handles>,
) {
    // table
    handles.table_shape = Cylinder::new(2.0, 2.0);
    handles.table_mesh = meshes.add(handles.table_shape.clone());
    handles.table_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.8, 0.7, 0.6),
        ..default()
    });

    // deck
    handles.deck_shape = Cuboid::new(0.5, 0.1, 0.7);
    handles.deck_mesh = meshes.add(handles.deck_shape.clone());
    handles.deck_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.0, 0.0, 0.0),
        ..default()
    });

    // player
    handles.player_shape = Capsule3d::new(0.2, 0.5);
    handles.player_mesh = meshes.add(handles.player_shape.clone());
    handles.player_material = materials.add(StandardMaterial {
        base_color: Color::rgb(0.0, 0.0, 1.0),
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
            edge_pan_width: 0.1,
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
                base_color: Color::rgb(0.0, 0.5, 0.0),
                perceptual_roughness: 0.9,
                ..default()
            }),
            ..default()
        },
        Ground,
    ));

    // spawn the first table
    reset_events.send(SpawnTableEvent { num_players: 5 });
    reset_events.send(SpawnTableEvent { num_players: 4 });
    reset_events.send(SpawnTableEvent { num_players: 3 });
    reset_events.send(SpawnTableEvent { num_players: 2 });
    reset_events.send(SpawnTableEvent { num_players: 1 });
}

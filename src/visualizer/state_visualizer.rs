use std::any::type_name;
use crate::cards::Card;
use crate::cards::Rank;
use crate::cards::Suit;
use crate::players::PlayerId;
use crate::state::State; // <--- your game State struct
use eyre::Context;
use macroquad::prelude::*;
use std::collections::HashMap;
use futures::future::join_all;
use strum::VariantArray;

pub struct StateVisualizer {
    /// A texture handle for each suit+rank combination
    card_textures: HashMap<(Suit, Rank), Texture2D>,
    /// A texture for face-down cards
    card_back: Texture2D,
    /// If present, we show that player's hand face-up and others face-down
    pub spectating: Option<PlayerId>,
}

impl StateVisualizer {
    /// Load all the playing card textures, plus back.png, into a new `StateVisualizer`.
    /// Adjust the path logic as needed.
    pub async fn new(spectating: Option<PlayerId>) -> eyre::Result<Self> {
        let this = type_name::<StateVisualizer>();
        println!("Constructing {}", this);
        // We'll build a small helper to load a single texture from disk
        async fn load_card_png(path: &str) -> eyre::Result<Texture2D> {
            // Expect fails if it cannot load. You may want your own error-handling in real code
            let tex = load_texture(path)
                .await
                .wrap_err(format!("Unable to load {}", path))?;
            tex.set_filter(FilterMode::Nearest);
            Ok(tex)
        }

        // Build a mapping from (Suit, Rank) -> texture path
        let mut card_textures = HashMap::new();
        let card_assets_dir = "cards";
        let card_to_file = |suit: Suit, rank: Rank| {
            format!(
                "{card_assets_dir}/{}_{}_white.png",
                suit,
                match rank {
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
                    Rank::Ace => "A",
                }
            )
        };

        println!("Loading card images for {}", this);
        let tasks: Vec<_> = Suit::VARIANTS.iter().flat_map(|suit| {
            Rank::VARIANTS.iter().map(move |rank| {
                let filename = card_to_file(*suit, *rank);
                async move {
                    let tex = load_card_png(&filename).await?;
                    Ok(((*suit, *rank), tex)) as eyre::Result<((Suit, Rank), Texture2D)>
                }
            })
        }).collect();
        
        let results: Vec<_> = join_all(tasks).await.into_iter().collect::<Result<_, _>>()?;
        for (key, tex) in results {
            card_textures.insert(key, tex);
        }
        println!("Finished loading card images for {}", this);

        // Also load the back of the card
        let card_back = load_card_png(&format!("{card_assets_dir}/back.png")).await?;
        println!("Finished constructing {}", this);

        Ok(StateVisualizer {
            card_textures,
            card_back,
            spectating,
        })
    }

    /// Draw the entire state to the screen.
    /// You can call this in your main game loop each frame.
    pub fn draw(&self, state: &State) -> eyre::Result<()> {
        // Example: clear the background
        clear_background(WHITE);

        // We can show some textual info:
        draw_text(
            &format!("Pot: {}   Round: {}", state.pot, state.round.round_number),
            20.0,
            20.0,
            24.0,
            BLACK,
        );

        // Let's place each player in a row (very simplistic!)
        // We'll compute a y offset for each player's row
        // (Better would be to place them around a circle, etc.)
        let mut y_offset = 60.0;
        for player in state.players.iter() {
            let name_str = &player.id.0;
            // Draw their name & score
            draw_text(
                &format!("{} (score = {})", name_str, player.score),
                20.0,
                y_offset,
                24.0,
                DARKGRAY,
            );

            // Render that player's hand
            y_offset += 30.0;
            self.draw_player_hand(player, 20.0, y_offset);

            // Add more vertical spacing
            y_offset += 110.0;
        }
        Ok(())
    }

    /// Renders a single player's hand as a row of cards, starting at (x,y).
    fn draw_player_hand(&self, player: &crate::players::Player, x: f32, y: f32) {
        let is_spectated = self
            .spectating
            .as_ref()
            .map_or(false, |spectator_id| *spectator_id == player.id);

        let mut x_cursor = x;
        for card in player.hand.iter() {
            if is_spectated {
                // Show the real card
                self.draw_card(card, x_cursor, y);
            } else {
                // Show a card back
                self.draw_card_back(x_cursor, y);
            }
            x_cursor += 40.0; // Overlap offset for each card
        }
    }

    fn draw_card(&self, card: &Card, x: f32, y: f32) {
        // If we have the correct suit/rank texture, draw it
        if let Some(tex) = self.card_textures.get(&(card.suit, card.rank)) {
            let w = tex.width();
            let h = tex.height();
            // We'll just scale them down to some arbitrary size
            let scale = 0.6;
            draw_texture_ex(
                tex,
                x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(w * scale, h * scale)),
                    ..Default::default()
                },
            );
        } else {
            // If we can't find the texture, just draw a red rectangle
            draw_rectangle(x, y, 32., 48., RED);
        }
    }

    fn draw_card_back(&self, x: f32, y: f32) {
        let tex = &self.card_back;
        let scale = 0.6;
        draw_texture_ex(
            tex,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(tex.width() * scale, tex.height() * scale)),
                ..Default::default()
            },
        );
    }
}

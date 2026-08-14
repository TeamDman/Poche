use facet::Facet;
use figue as args;
use poche_domain::parse_card_name;
use poche_protocol::GameActionWire;

use super::super::ParseError;

#[derive(Facet, PartialEq, Eq)]
pub struct GameArgs {
    #[facet(args::subcommand)]
    pub command: GameCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum GameCommand {
    Observe {
        #[facet(args::positional)]
        room: String,
    },
    Actions {
        #[facet(args::positional)]
        room: String,
    },
    Bid {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        tricks: u8,
    },
    PlayCard {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        card: String,
    },
}

impl GameArgs {
    pub(crate) fn validate(&self) -> Result<(), ParseError> {
        if let GameCommand::PlayCard { card, .. } = &self.command {
            parse_card_name(card)
                .map_err(|_| ParseError::new("card must be rank-suit; use game --help"))?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            GameCommand::Observe { .. } => "observe",
            GameCommand::Actions { .. } => "actions",
            GameCommand::Bid { .. } => "bid",
            GameCommand::PlayCard { .. } => "play-card",
        }
    }

    /// Return the typed game action carried by a dedicated action command.
    #[must_use]
    pub fn game_action(&self) -> Option<GameActionWire> {
        match &self.command {
            GameCommand::Bid { tricks, .. } => Some(GameActionWire::Bid { tricks: *tricks }),
            GameCommand::PlayCard { card, .. } => parse_card_name(card)
                .ok()
                .map(|card| GameActionWire::Play { card: card.code() }),
            GameCommand::Observe { .. } | GameCommand::Actions { .. } => None,
        }
    }

    /// Execute observation/action queries and typed game actions through one
    /// certified external device client.
    ///
    /// # Errors
    ///
    /// Returns a redacted profile, transport, observation, action, or output
    /// error.
    pub fn invoke(
        self,
        config: &crate::cli::live_device::LiveDeviceConfig,
        output: crate::cli::output::OutputFormat,
    ) -> eyre::Result<bool> {
        match self.command {
            GameCommand::Observe { room } => config.observe(&room, 1, output),
            GameCommand::Actions { room } => config.actions(&room, 1, output),
            GameCommand::Bid { room, tricks } => config.invoke_payload(
                &room,
                1,
                &poche_protocol::CommandPayload::GameAction {
                    action: GameActionWire::Bid { tricks },
                },
                "game-bid",
                output,
            ),
            GameCommand::PlayCard { room, card } => {
                let card = parse_card_name(&card)
                    .map_err(|_| eyre::eyre!("card must be a canonical rank-suit name"))?;
                config.invoke_payload(
                    &room,
                    1,
                    &poche_protocol::CommandPayload::GameAction {
                        action: GameActionWire::Play { card: card.code() },
                    },
                    "game-play",
                    output,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::GameActionWire;

    use super::{GameArgs, GameCommand};

    #[test]
    fn play_card_validates_canonical_name_to_typed_wire_action() {
        let parsed = GameArgs {
            command: GameCommand::PlayCard {
                room: "room-1".to_owned(),
                card: "jack-spades".to_owned(),
            },
        };
        parsed.validate().expect("canonical card command");
        assert_eq!(
            parsed.game_action(),
            Some(GameActionWire::Play { card: 48 })
        );
        let invalid = GameArgs {
            command: GameCommand::PlayCard {
                room: "room-1".to_owned(),
                card: "J-spades".to_owned(),
            },
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn bid_is_a_typed_game_action() {
        let parsed = GameArgs {
            command: GameCommand::Bid {
                room: "room-1".to_owned(),
                tricks: 3,
            },
        };
        assert_eq!(
            parsed.game_action(),
            Some(GameActionWire::Bid { tricks: 3 })
        );
    }
}

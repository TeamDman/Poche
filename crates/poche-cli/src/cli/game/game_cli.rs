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
            GameCommand::PlayCard { .. } => "play-card",
        }
    }

    /// Return the typed game action carried by a dedicated action command.
    #[must_use]
    pub fn game_action(&self) -> Option<GameActionWire> {
        match &self.command {
            GameCommand::PlayCard { card, .. } => parse_card_name(card)
                .ok()
                .map(|card| GameActionWire::Play { card: card.code() }),
            GameCommand::Observe { .. } | GameCommand::Actions { .. } => None,
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
}

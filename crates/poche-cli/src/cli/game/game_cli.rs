use poche_domain::{CardId, parse_card_name};
use poche_protocol::GameActionWire;

use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum GameArgs {
    Observe { room: String },
    Actions { room: String },
    Act { room: String, action: String },
    PlayCard { room: String, card: CardId },
}

impl GameArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments
            .split_first()
            .ok_or_else(|| ParseError::new("game command is required; use game --help"))?;
        match command.as_str() {
            "observe" => Ok(Self::Observe {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "actions" => Ok(Self::Actions {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "act" => {
                let arguments = exact(arguments, 2)?;
                Ok(Self::Act {
                    room: arguments[0].clone(),
                    action: arguments[1].clone(),
                })
            }
            "play-card" => {
                let arguments = exact(arguments, 2)?;
                Ok(Self::PlayCard {
                    room: arguments[0].clone(),
                    card: parse_card_name(&arguments[1])
                        .map_err(|_| ParseError::new("card must be rank-suit; use game --help"))?,
                })
            }
            _ => Err(ParseError::new("unknown game command; use game --help")),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Observe { .. } => "observe",
            Self::Actions { .. } => "actions",
            Self::Act { .. } => "act",
            Self::PlayCard { .. } => "play-card",
        }
    }

    /// Return the typed game action carried by a dedicated action command.
    #[must_use]
    pub const fn game_action(&self) -> Option<GameActionWire> {
        match self {
            Self::PlayCard { card, .. } => Some(GameActionWire::Play { card: card.code() }),
            Self::Observe { .. } | Self::Actions { .. } | Self::Act { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::GameActionWire;

    use super::GameArgs;

    #[test]
    fn play_card_parses_canonical_name_to_typed_wire_action() {
        let parsed = GameArgs::parse(&[
            "play-card".to_owned(),
            "room-1".to_owned(),
            "jack-spades".to_owned(),
        ])
        .expect("canonical card command");
        assert_eq!(
            parsed.game_action(),
            Some(GameActionWire::Play { card: 48 })
        );
        assert!(
            GameArgs::parse(&[
                "play-card".to_owned(),
                "room-1".to_owned(),
                "J-spades".to_owned(),
            ])
            .is_err()
        );
    }
}

use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum SpectatorArgs {
    RequestHand { room: String, player: String },
    GrantHand { room: String, spectator: String },
    RevokeHand { room: String, spectator: String },
}

impl SpectatorArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments.split_first().ok_or_else(|| {
            ParseError::new("spectator command is required; use spectator --help")
        })?;
        let arguments = exact(arguments, 2)?;
        match command.as_str() {
            "request-hand" => Ok(Self::RequestHand {
                room: arguments[0].clone(),
                player: arguments[1].clone(),
            }),
            "grant-hand" => Ok(Self::GrantHand {
                room: arguments[0].clone(),
                spectator: arguments[1].clone(),
            }),
            "revoke-hand" => Ok(Self::RevokeHand {
                room: arguments[0].clone(),
                spectator: arguments[1].clone(),
            }),
            _ => Err(ParseError::new(
                "unknown spectator command; use spectator --help",
            )),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RequestHand { .. } => "request-hand",
            Self::GrantHand { .. } => "grant-hand",
            Self::RevokeHand { .. } => "revoke-hand",
        }
    }
}

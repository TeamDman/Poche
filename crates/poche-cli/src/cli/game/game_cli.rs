use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum GameArgs {
    Observe { room: String },
    Actions { room: String },
    Act { room: String, action: String },
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
            _ => Err(ParseError::new("unknown game command; use game --help")),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Observe { .. } => "observe",
            Self::Actions { .. } => "actions",
            Self::Act { .. } => "act",
        }
    }
}

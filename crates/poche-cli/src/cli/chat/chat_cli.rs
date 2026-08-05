use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum ChatArgs {
    Send { room: String, message: String },
    Tail { room: String, limit: usize },
}

impl ChatArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments
            .split_first()
            .ok_or_else(|| ParseError::new("chat command is required; use chat --help"))?;
        match command.as_str() {
            "send" => {
                let arguments = exact(arguments, 2)?;
                Ok(Self::Send {
                    room: arguments[0].clone(),
                    message: arguments[1].clone(),
                })
            }
            "tail" => {
                let arguments = exact(arguments, 2)?;
                Ok(Self::Tail {
                    room: arguments[0].clone(),
                    limit: arguments[1]
                        .parse()
                        .map_err(|_| ParseError::new("chat tail limit must be an integer"))?,
                })
            }
            _ => Err(ParseError::new("unknown chat command; use chat --help")),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Send { .. } => "send",
            Self::Tail { .. } => "tail",
        }
    }
}

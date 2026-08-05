use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum TranscriptArgs {
    Record { path: String },
    Replay { path: String },
    Inspect { path: String },
}

impl TranscriptArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments.split_first().ok_or_else(|| {
            ParseError::new("transcript command is required; use transcript --help")
        })?;
        let path = exact(arguments, 1)?[0].clone();
        match command.as_str() {
            "record" => Ok(Self::Record { path }),
            "replay" => Ok(Self::Replay { path }),
            "inspect" => Ok(Self::Inspect { path }),
            _ => Err(ParseError::new(
                "unknown transcript command; use transcript --help",
            )),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Record { .. } => "record",
            Self::Replay { .. } => "replay",
            Self::Inspect { .. } => "inspect",
        }
    }
}

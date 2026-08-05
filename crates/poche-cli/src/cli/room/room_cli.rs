use super::super::{ParseError, exact};

/// Room lifecycle commands. Invite material is intentionally held only by the
/// join variant and is never exposed by command naming or output.
#[derive(PartialEq, Eq)]
pub enum RoomArgs {
    Host,
    Join { invite_code: String },
    Show { room: String },
    Ready { room: String },
    Unready { room: String },
    Countdown { room: String, ticks: u64 },
    Abort { room: String },
    Pause { room: String },
    Resume { room: String },
    Leave { room: String },
    Close { room: String },
}

impl RoomArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments
            .split_first()
            .ok_or_else(|| ParseError::new("room command is required; use room --help"))?;
        match command.as_str() {
            "host" => {
                exact(arguments, 0)?;
                Ok(Self::Host)
            }
            "join" => Ok(Self::Join {
                invite_code: exact(arguments, 1)?[0].clone(),
            }),
            "show" => Ok(Self::Show {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "ready" => Ok(Self::Ready {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "unready" => Ok(Self::Unready {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "countdown" => {
                let arguments = exact(arguments, 2)?;
                Ok(Self::Countdown {
                    room: arguments[0].clone(),
                    ticks: arguments[1]
                        .parse()
                        .map_err(|_| ParseError::new("countdown ticks must be an integer"))?,
                })
            }
            "abort" => Ok(Self::Abort {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "pause" => Ok(Self::Pause {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "resume" => Ok(Self::Resume {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "leave" => Ok(Self::Leave {
                room: exact(arguments, 1)?[0].clone(),
            }),
            "close" => Ok(Self::Close {
                room: exact(arguments, 1)?[0].clone(),
            }),
            _ => Err(ParseError::new("unknown room command; use room --help")),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Join { .. } => "join",
            Self::Show { .. } => "show",
            Self::Ready { .. } => "ready",
            Self::Unready { .. } => "unready",
            Self::Countdown { .. } => "countdown",
            Self::Abort { .. } => "abort",
            Self::Pause { .. } => "pause",
            Self::Resume { .. } => "resume",
            Self::Leave { .. } => "leave",
            Self::Close { .. } => "close",
        }
    }
}

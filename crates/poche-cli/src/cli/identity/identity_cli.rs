use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum IdentityArgs {
    Show,
    Create { label: String },
}

impl IdentityArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments
            .split_first()
            .ok_or_else(|| ParseError::new("identity command is required; use identity --help"))?;
        match command.as_str() {
            "show" => {
                exact(arguments, 0)?;
                Ok(Self::Show)
            }
            "create" => Ok(Self::Create {
                label: exact(arguments, 1)?[0].clone(),
            }),
            _ => Err(ParseError::new(
                "unknown identity command; use identity --help",
            )),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Create { .. } => "create",
        }
    }
}

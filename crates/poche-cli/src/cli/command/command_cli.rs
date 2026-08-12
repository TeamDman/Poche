// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Small deterministic slash parser for the Facet-reflected protocol AST.

use eyre::{Context, Result};
use facet::Facet;
use figue as args;
use poche_domain::parse_card_name;
use poche_protocol::{
    EventId, GameActionWire, GovernanceCapabilityWire, GovernanceCommandV1, GovernanceCommandWire,
    GovernedActionWire, PrincipalId, ProposalId, RecoveryActionWire, RightsChangeWire,
    VoteChoiceWire,
};

use super::super::ParseError;
use crate::cli::output::{OutputFormat, write_stdout};

const COMMAND_NAMES: [&str; 8] = [
    "play-card",
    "bid",
    "score",
    "rights",
    "accuse",
    "recovery",
    "startvote",
    "vote",
];

const COMMAND_FORMS: [&str; 13] = [
    "/play-card <rank-suit>",
    "/bid <tricks>",
    "/score add|remove <player> <points>",
    "/rights grant|remove <player> adjust-score|change-rights|resolve-recovery",
    "/accuse <event-id>",
    "/recovery redeal",
    "/recovery kick <player>",
    "/recovery end-game",
    "/startvote \"<typed-action>\"",
    "/vote <proposal-id> approve",
    "/vote <proposal-id> reject",
    "/vote <proposal-id> abstain",
    "structural card mutation is not a command",
];

/// Top-level client command for inspecting one typed slash-command parse.
#[derive(Facet, PartialEq, Eq)]
pub struct CommandArgs {
    #[facet(args::subcommand)]
    pub command: CommandCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum CommandCommand {
    Parse {
        #[facet(args::positional, sensitive)]
        command: String,
    },
}

impl CommandArgs {
    pub(crate) fn validate(&self) -> Result<(), ParseError> {
        let CommandCommand::Parse { command } = &self.command;
        parse_slash_command(command).map(|_| ())
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        "parse"
    }

    /// Parse and return the validated typed command without retaining source
    /// text in the result.
    ///
    /// # Errors
    ///
    /// Returns a redacted parse error for an invalid typed slash command.
    pub fn typed_command(&self) -> Result<GovernanceCommandV1, ParseError> {
        let CommandCommand::Parse { command } = &self.command;
        parse_slash_command(command)
    }

    /// Print the validated AST without retaining or executing its source text.
    ///
    /// # Errors
    ///
    /// Returns a JSON serialization or stdout error.
    pub fn invoke(self, format: OutputFormat) -> Result<bool> {
        let CommandCommand::Parse { command } = self.command;
        let command = parse_slash_command(&command).map_err(|error| eyre::eyre!(error))?;
        let encoded = match format {
            OutputFormat::Text | OutputFormat::Json => {
                serde_json::to_string_pretty(&command).wrap_err("failed to encode typed command")?
            }
            OutputFormat::Ndjson => {
                serde_json::to_string(&command).wrap_err("failed to encode typed command")?
            }
        };
        write_stdout(&format!("{encoded}\n"))?;
        Ok(true)
    }
}

/// Parse one slash command into the protocol AST. No source string survives a
/// successful parse and nothing in this module invokes a shell or dispatcher.
///
/// # Errors
///
/// Returns a value-free diagnostic for malformed or unsupported input.
pub fn parse_slash_command(line: &str) -> Result<GovernanceCommandV1, ParseError> {
    let tokens = tokenize(line)?;
    GovernanceCommandV1::new(parse_tokens(&tokens)?)
        .map_err(|_| ParseError::new("slash command violates the typed command contract"))
}

/// Generate stable help from the same command-form catalog as the parser.
#[must_use]
pub fn slash_command_help() -> String {
    format!(
        "Poche typed commands (poche-governance-command-v1)\n\nUSAGE:\n{}\n\nCommands are converted to the Facet-reflected protocol AST before signing.\n",
        COMMAND_FORMS
            .iter()
            .map(|form| format!("  {form}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Shell families supported by the deterministic completion adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

/// Generate a shell completion script from the same command-name catalog.
#[must_use]
pub fn slash_command_completions(shell: CompletionShell) -> String {
    let commands = COMMAND_NAMES.join(" ");
    match shell {
        CompletionShell::Bash => format!("complete -W \"{commands}\" poche-command\n"),
        CompletionShell::Zsh => format!("compctl -k \"({commands})\" poche-command\n"),
        CompletionShell::Fish => COMMAND_NAMES
            .iter()
            .map(|command| format!("complete -c poche-command -a '{command}'"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn parse_tokens(tokens: &[String]) -> Result<GovernanceCommandWire, ParseError> {
    let token = |index| tokens.get(index).map(String::as_str);
    let execute = |action| GovernanceCommandWire::Execute { action };
    let command = match tokens.len() {
        2 if token(0) == Some("play-card") => execute(GovernedActionWire::Game {
            action: GameActionWire::Play {
                card: parse_card_name(token(1).unwrap_or_default())
                    .map_err(|_| ParseError::new("card must be canonical rank-suit"))?
                    .code(),
            },
        }),
        2 if token(0) == Some("bid") => execute(GovernedActionWire::Game {
            action: GameActionWire::Bid {
                tricks: parse_u8(token(1))?,
            },
        }),
        4 if token(0) == Some("score") && matches!(token(1), Some("add" | "remove")) => {
            let target = principal(token(2), "score player identifier is invalid")?;
            let points = parse_positive_i32(token(3))?;
            let delta = if token(1) == Some("add") {
                points
            } else {
                -points
            };
            execute(GovernedActionWire::AdjustScore { target, delta })
        }
        4 if token(0) == Some("rights") && matches!(token(1), Some("grant" | "remove")) => {
            execute(GovernedActionWire::ChangeRights {
                target: principal(token(2), "rights player identifier is invalid")?,
                capability: capability(token(3))?,
                change: if token(1) == Some("grant") {
                    RightsChangeWire::Grant
                } else {
                    RightsChangeWire::Revoke
                },
            })
        }
        2 if token(0) == Some("accuse") => execute(GovernedActionWire::Accuse {
            offending_event_id: EventId::new(token(1).unwrap_or_default())
                .map_err(|_| ParseError::new("accusation event identifier is invalid"))?,
        }),
        2 if token(0) == Some("recovery") && token(1) == Some("redeal") => {
            execute(GovernedActionWire::Recover {
                recovery: RecoveryActionWire::Redeal,
            })
        }
        3 if token(0) == Some("recovery") && token(1) == Some("kick") => {
            execute(GovernedActionWire::Recover {
                recovery: RecoveryActionWire::Kick {
                    target: principal(token(2), "kick player identifier is invalid")?,
                },
            })
        }
        2 if token(0) == Some("recovery") && token(1) == Some("end-game") => {
            execute(GovernedActionWire::Recover {
                recovery: RecoveryActionWire::EndGame,
            })
        }
        2 if token(0) == Some("startvote") => {
            let nested = parse_slash_command(token(1).unwrap_or_default())?;
            let GovernanceCommandWire::Execute { action } = nested.command else {
                return Err(ParseError::new(
                    "a vote may wrap one executable action, not another vote",
                ));
            };
            GovernanceCommandWire::StartVote { action }
        }
        3 if token(0) == Some("vote") => GovernanceCommandWire::Vote {
            proposal_id: ProposalId::new(token(1).unwrap_or_default())
                .map_err(|_| ParseError::new("proposal identifier is invalid"))?,
            choice: match token(2) {
                Some("approve") => VoteChoiceWire::Approve,
                Some("reject") => VoteChoiceWire::Reject,
                Some("abstain") => VoteChoiceWire::Abstain,
                _ => return Err(ParseError::new("vote choice is invalid")),
            },
        },
        _ => return Err(ParseError::new("invalid slash command; use command --help")),
    };
    Ok(command)
}

fn principal(value: Option<&str>, error: &'static str) -> Result<PrincipalId, ParseError> {
    PrincipalId::new(value.unwrap_or_default()).map_err(|_| ParseError::new(error))
}

fn parse_u8(value: Option<&str>) -> Result<u8, ParseError> {
    value
        .unwrap_or_default()
        .parse()
        .map_err(|_| ParseError::new("bid must be a whole number"))
}

fn parse_positive_i32(value: Option<&str>) -> Result<i32, ParseError> {
    let value = value
        .unwrap_or_default()
        .parse()
        .map_err(|_| ParseError::new("score points must be a positive integer"))?;
    if value > 0 {
        Ok(value)
    } else {
        Err(ParseError::new("score points must be a positive integer"))
    }
}

fn capability(value: Option<&str>) -> Result<GovernanceCapabilityWire, ParseError> {
    match value {
        Some("adjust-score") => Ok(GovernanceCapabilityWire::AdjustScore),
        Some("change-rights") => Ok(GovernanceCapabilityWire::ChangeRights),
        Some("resolve-recovery") => Ok(GovernanceCapabilityWire::ResolveRecovery),
        _ => Err(ParseError::new("governance capability is invalid")),
    }
}

fn tokenize(line: &str) -> Result<Vec<String>, ParseError> {
    let line = line.trim();
    let Some(line) = line.strip_prefix('/') else {
        return Err(ParseError::new("slash command must begin with '/'"));
    };
    if line.is_empty() || line.chars().any(char::is_control) {
        return Err(ParseError::new(
            "slash command contains invalid control text",
        ));
    }
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            '\\' => {
                return Err(ParseError::new(
                    "slash commands do not accept escape sequences",
                ));
            }
            character if character.is_whitespace() && !quoted => {
                if !token.is_empty() {
                    tokens.push(core::mem::take(&mut token));
                }
            }
            _ => token.push(character),
        }
    }
    if quoted || token.is_empty() && tokens.is_empty() {
        return Err(ParseError::new("slash command has invalid quoting"));
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_examples_have_one_typed_meaning() {
        let play = parse_slash_command("/play-card jack-spades").unwrap();
        assert_eq!(
            play.command,
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::Game {
                    action: GameActionWire::Play { card: 48 }
                }
            }
        );
        let score = parse_slash_command("/score add player1 100").unwrap();
        assert_eq!(
            score.command,
            GovernanceCommandWire::Execute {
                action: GovernedActionWire::AdjustScore {
                    target: PrincipalId::new("player1").unwrap(),
                    delta: 100
                }
            }
        );
        let vote = parse_slash_command("/startvote \"/score add player1 100\"").unwrap();
        assert_eq!(
            vote.command,
            GovernanceCommandWire::StartVote {
                action: GovernedActionWire::AdjustScore {
                    target: PrincipalId::new("player1").unwrap(),
                    delta: 100
                }
            }
        );
    }

    #[test]
    fn command_rights_accusation_recovery_and_vote_parse() {
        for command in [
            "/rights remove player1 adjust-score",
            "/accuse event-2",
            "/recovery kick player1",
            "/vote proposal-1 approve",
        ] {
            parse_slash_command(command)
                .unwrap_or_else(|error| panic!("{command} should parse: {error}"));
        }
    }

    #[test]
    fn command_help_and_completions_share_the_command_catalog() {
        let help = slash_command_help();
        for command in [
            "play-card",
            "score",
            "rights",
            "accuse",
            "startvote",
            "vote",
        ] {
            assert!(help.contains(command), "help omitted {command}");
        }
        let completions = slash_command_completions(CompletionShell::Bash);
        assert!(completions.contains("play-card"));
        assert!(completions.contains("startvote"));
    }

    #[test]
    fn command_text_never_becomes_a_raw_executable_payload() {
        assert!(parse_slash_command("score add player1 100").is_err());
        assert!(parse_slash_command("/startvote \"/startvote /score\"").is_err());
        assert!(parse_slash_command("/score add player1 0").is_err());
        assert!(parse_slash_command("/card create ace-spades").is_err());
        assert!(parse_slash_command("/score add player1 100\\n/recovery end-game").is_err());
    }
}

//! Strict typed CLI schema and parser.

pub mod chat;
pub mod game;
pub mod identity;
pub mod output;
pub mod room;
pub mod spectator;
pub mod transcript;

use core::fmt;

use chat::ChatArgs;
use game::GameArgs;
use identity::IdentityArgs;
use output::OutputFormat;
use room::RoomArgs;
use spectator::SpectatorArgs;
use transcript::TranscriptArgs;

/// Global process controls. Values are never logged as a whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalArgs {
    pub debug: bool,
    pub log_filter: Option<String>,
    pub log_file: Option<String>,
    pub output: OutputFormat,
    pub stop_after_ms: Option<u64>,
}

/// Fully parsed invocation.
#[derive(PartialEq, Eq)]
pub struct Cli {
    pub global: GlobalArgs,
    pub command: Command,
}

/// Top-level Poche command groups.
#[derive(PartialEq, Eq)]
pub enum Command {
    Room(RoomArgs),
    Game(GameArgs),
    Chat(ChatArgs),
    Spectator(SpectatorArgs),
    Transcript(TranscriptArgs),
    Identity(IdentityArgs),
}

impl Command {
    #[must_use]
    pub const fn name(&self) -> (&'static str, &'static str) {
        match self {
            Self::Room(command) => ("room", command.name()),
            Self::Game(command) => ("game", command.name()),
            Self::Chat(command) => ("chat", command.name()),
            Self::Spectator(command) => ("spectator", command.name()),
            Self::Transcript(command) => ("transcript", command.name()),
            Self::Identity(command) => ("identity", command.name()),
        }
    }
}

/// Non-executing outcomes handled before runtime/log initialization.
#[derive(PartialEq, Eq)]
pub enum ParseOutcome {
    Help(Vec<String>),
    Version,
    Run(Cli),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError(&'static str);

impl ParseError {
    pub(crate) const fn new(message: &'static str) -> Self {
        Self(message)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parse strict Unicode arguments without exiting the process.
///
/// Raw values are intentionally absent from parse errors so invite material is
/// not copied into diagnostics.
///
/// # Errors
///
/// Returns a value-free diagnostic for malformed or unsupported arguments.
pub fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<ParseOutcome, ParseError> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let mut index = 0;
    let mut global = GlobalArgs::default();
    while let Some(argument) = arguments.get(index) {
        match argument.as_str() {
            "--debug" => global.debug = true,
            "--log-filter" => {
                index += 1;
                global.log_filter = Some(
                    arguments
                        .get(index)
                        .ok_or_else(|| ParseError::new("--log-filter requires a value"))?
                        .clone(),
                );
            }
            "--log-file" => {
                index += 1;
                global.log_file = Some(
                    arguments
                        .get(index)
                        .ok_or_else(|| ParseError::new("--log-file requires a path"))?
                        .clone(),
                );
            }
            "--output" => {
                index += 1;
                global.output = arguments
                    .get(index)
                    .ok_or_else(|| ParseError::new("--output requires text, json, or ndjson"))?
                    .parse()?;
            }
            "--stop-after-ms" => {
                index += 1;
                global.stop_after_ms = Some(
                    arguments
                        .get(index)
                        .ok_or_else(|| ParseError::new("--stop-after-ms requires milliseconds"))?
                        .parse()
                        .map_err(|_| ParseError::new("--stop-after-ms must be an integer"))?,
                );
            }
            "--help" | "-h" => return Ok(ParseOutcome::Help(Vec::new())),
            "--version" | "-V" => return Ok(ParseOutcome::Version),
            value if value.starts_with('-') => {
                return Err(ParseError::new("unknown global option"));
            }
            _ => break,
        }
        index += 1;
    }

    let command_arguments = &arguments[index..];
    if let Some(help_index) = command_arguments
        .iter()
        .position(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        return Ok(ParseOutcome::Help(
            command_arguments[..help_index]
                .iter()
                .take(2)
                .cloned()
                .collect(),
        ));
    }

    let (group, rest) = command_arguments
        .split_first()
        .ok_or_else(|| ParseError::new("a command group is required; use --help"))?;
    let command = match group.as_str() {
        "room" => Command::Room(RoomArgs::parse(rest)?),
        "game" => Command::Game(GameArgs::parse(rest)?),
        "chat" => Command::Chat(ChatArgs::parse(rest)?),
        "spectator" => Command::Spectator(SpectatorArgs::parse(rest)?),
        "transcript" => Command::Transcript(TranscriptArgs::parse(rest)?),
        "identity" => Command::Identity(IdentityArgs::parse(rest)?),
        _ => return Err(ParseError::new("unknown command group; use --help")),
    };
    Ok(ParseOutcome::Run(Cli { global, command }))
}

#[must_use]
pub fn help(path: &[String]) -> String {
    let heading = if path.is_empty() {
        "Poche multiplayer and model-inspection CLI".to_owned()
    } else {
        format!("Poche help for {}", path.join(" "))
    };
    format!(
        "{heading}\n\nUSAGE:\n  poche [GLOBAL OPTIONS] <GROUP> <COMMAND> [ARGS]\n\nGLOBAL OPTIONS:\n  --debug\n  --log-filter <DIRECTIVES>\n  --log-file <NDJSON-PATH>\n  --output <text|json|ndjson>\n  --stop-after-ms <MILLISECONDS>\n  --help\n  --version\n\nCOMMANDS:\n  room host|join|show|ready|unready|countdown|abort|pause|resume|leave|close\n  game observe|actions|act|play-card\n  chat send|tail\n  spectator request-hand|grant-hand|revoke-hand\n  transcript record|replay|inspect\n  identity show|create\n"
    )
}

pub(crate) fn exact(arguments: &[String], count: usize) -> Result<&[String], ParseError> {
    if arguments.len() == count {
        Ok(arguments)
    } else {
        Err(ParseError::new(
            "wrong number of command arguments; use --help",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_and_typed_command() {
        let parsed = parse_args([
            "--output".to_owned(),
            "json".to_owned(),
            "room".to_owned(),
            "ready".to_owned(),
            "room-1".to_owned(),
        ])
        .expect("arguments should parse");
        let ParseOutcome::Run(parsed) = parsed else {
            panic!("expected runnable command");
        };
        assert_eq!(parsed.global.output, OutputFormat::Json);
        assert_eq!(parsed.command.name(), ("room", "ready"));
    }

    #[test]
    fn errors_never_echo_raw_values() {
        let secret = "invite-secret-never-repeat";
        let error = parse_args([
            "room".to_owned(),
            "join".to_owned(),
            secret.to_owned(),
            "extra".to_owned(),
        ])
        .err()
        .expect("extra argument should be rejected")
        .to_string();
        assert!(!error.contains(secret));
    }

    #[test]
    fn every_declared_command_round_trips_through_the_schema() {
        let cases: &[(&[&str], (&str, &str))] = &[
            (&["room", "host"], ("room", "host")),
            (&["room", "join", "invite"], ("room", "join")),
            (&["room", "show", "r"], ("room", "show")),
            (&["room", "ready", "r"], ("room", "ready")),
            (&["room", "unready", "r"], ("room", "unready")),
            (&["room", "countdown", "r", "3"], ("room", "countdown")),
            (&["room", "abort", "r"], ("room", "abort")),
            (&["room", "pause", "r"], ("room", "pause")),
            (&["room", "resume", "r"], ("room", "resume")),
            (&["room", "leave", "r"], ("room", "leave")),
            (&["room", "close", "r"], ("room", "close")),
            (&["game", "observe", "r"], ("game", "observe")),
            (&["game", "actions", "r"], ("game", "actions")),
            (&["game", "act", "r", "pass"], ("game", "act")),
            (
                &["game", "play-card", "r", "jack-spades"],
                ("game", "play-card"),
            ),
            (&["chat", "send", "r", "hello"], ("chat", "send")),
            (&["chat", "tail", "r", "20"], ("chat", "tail")),
            (
                &["spectator", "request-hand", "r", "p"],
                ("spectator", "request-hand"),
            ),
            (
                &["spectator", "grant-hand", "r", "s"],
                ("spectator", "grant-hand"),
            ),
            (
                &["spectator", "revoke-hand", "r", "s"],
                ("spectator", "revoke-hand"),
            ),
            (
                &["transcript", "record", "events.ndjson"],
                ("transcript", "record"),
            ),
            (
                &["transcript", "replay", "events.ndjson"],
                ("transcript", "replay"),
            ),
            (
                &["transcript", "inspect", "events.ndjson"],
                ("transcript", "inspect"),
            ),
            (&["identity", "show"], ("identity", "show")),
            (&["identity", "create", "alice"], ("identity", "create")),
        ];
        for (arguments, expected) in cases {
            let outcome = parse_args(arguments.iter().map(ToString::to_string))
                .unwrap_or_else(|error| panic!("{arguments:?}: {error}"));
            let ParseOutcome::Run(cli) = outcome else {
                panic!("{arguments:?}: expected runnable command");
            };
            assert_eq!(&cli.command.name(), expected, "{arguments:?}");
        }
    }

    #[test]
    fn arbitrary_token_sequences_never_panic() {
        let vocabulary = [
            "",
            "--",
            "--debug",
            "--output",
            "json",
            "text",
            "--log-file",
            "room",
            "join",
            "game",
            "act",
            "chat",
            "send",
            "spectator",
            "grant-hand",
            "transcript",
            "replay",
            "identity",
            "show",
            "\0",
            "秘密",
            "999999999999999999999999999",
        ];
        let mut state = 0x5eed_cafe_f00d_beefu64;
        for _ in 0..10_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let length = usize::try_from(state % 12).expect("bounded length should fit usize");
            let mut arguments = Vec::with_capacity(length);
            for offset in 0..length {
                let rotation = u32::try_from(offset).expect("bounded rotation should fit u32");
                let vocabulary_len =
                    u64::try_from(vocabulary.len()).expect("vocabulary length should fit u64");
                let index = usize::try_from(state.rotate_left(rotation) % vocabulary_len)
                    .expect("bounded vocabulary index should fit usize");
                arguments.push(vocabulary[index].to_owned());
            }
            let result = std::panic::catch_unwind(|| parse_args(arguments));
            assert!(result.is_ok(), "parser panicked for arbitrary input");
        }
    }
}

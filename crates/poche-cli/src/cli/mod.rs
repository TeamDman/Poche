//! Facet-reflected, strict Poche CLI schema.

pub mod agent;
pub mod chat;
pub mod command;
pub mod desktop;
pub mod device;
pub mod game;
pub mod identity;
pub mod output;
pub mod puppet;
pub mod room;
pub mod spectator;
pub mod transcript;

use core::fmt;

use agent::AgentArgs;
use chat::ChatArgs;
use command::CommandArgs;
use desktop::DesktopArgs;
use device::DeviceArgs;
use facet::Facet;
use figue::{self as args, Driver, DriverError, FigueBuiltins};
use game::GameArgs;
use identity::IdentityArgs;
use output::OutputFormat;
use puppet::PuppetArgs;
use room::RoomArgs;
use spectator::SpectatorArgs;
use transcript::TranscriptArgs;

/// Global process controls. Values are never logged as a whole.
#[derive(Facet, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
pub struct GlobalArgs {
    #[facet(args::named, default)]
    pub debug: bool,
    #[facet(args::named)]
    pub log_filter: Option<String>,
    #[facet(args::named)]
    pub log_file: Option<String>,
    #[facet(args::named, default)]
    pub output: OutputFormat,
    #[facet(args::named)]
    pub stop_after_ms: Option<u64>,
}

/// Fully parsed invocation.
#[derive(Facet)]
#[facet(rename_all = "kebab-case")]
pub struct Cli {
    #[facet(flatten)]
    pub global: GlobalArgs,
    #[facet(flatten)]
    pub builtins: FigueBuiltins,
    #[facet(args::subcommand)]
    pub command: Command,
}

impl PartialEq for Cli {
    fn eq(&self, other: &Self) -> bool {
        self.global == other.global && self.command == other.command
    }
}

/// Top-level Poche command groups exposed by the one executable.
#[derive(Facet, PartialEq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Command {
    Desktop(DesktopArgs),
    Room(RoomArgs),
    Game(GameArgs),
    Chat(ChatArgs),
    #[facet(rename = "command")]
    Governance(CommandArgs),
    Spectator(SpectatorArgs),
    Transcript(TranscriptArgs),
    Identity(IdentityArgs),
    Agent(AgentArgs),
    Device(DeviceArgs),
    Puppet(PuppetArgs),
}

impl Command {
    #[must_use]
    pub const fn name(&self) -> (&'static str, &'static str) {
        match self {
            Self::Desktop(command) => ("desktop", command.name()),
            Self::Room(command) => ("room", command.name()),
            Self::Game(command) => ("game", command.name()),
            Self::Chat(command) => ("chat", command.name()),
            Self::Governance(command) => ("command", command.name()),
            Self::Spectator(command) => ("spectator", command.name()),
            Self::Transcript(command) => ("transcript", command.name()),
            Self::Identity(command) => ("identity", command.name()),
            Self::Agent(command) => ("agent", command.name()),
            Self::Device(command) => ("device", command.name()),
            Self::Puppet(command) => ("puppet", command.name()),
        }
    }

    fn validate(&self) -> Result<(), ParseError> {
        match self {
            Self::Game(command) => command.validate(),
            Self::Governance(command) => command.validate(),
            Self::Desktop(command) => {
                if command
                    .exit_after_seconds
                    .is_some_and(|seconds| !seconds.is_finite() || seconds < 1.0)
                {
                    Err(ParseError::new(
                        "--exit-after-seconds must be finite and at least one second",
                    ))
                } else {
                    Ok(())
                }
            }
            Self::Room(_)
            | Self::Chat(_)
            | Self::Spectator(_)
            | Self::Transcript(_)
            | Self::Identity(_)
            | Self::Agent(_)
            | Self::Device(_) => Ok(()),
            Self::Puppet(command) => command.validate(),
        }
    }
}

/// Non-executing outcomes handled before runtime/log initialization.
#[derive(PartialEq)]
pub enum ParseOutcome {
    Write(String),
    Run(Box<Cli>),
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

/// Parse strict Unicode arguments through Figue without exiting the process.
///
/// Raw values are intentionally absent from returned parse errors so invite
/// and command material cannot be copied into diagnostics. No arguments is an
/// explicit alias for `desktop`.
///
/// # Errors
///
/// Returns a value-free error for an invalid schema, command line, typed card,
/// or typed governance command.
pub fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<ParseOutcome, ParseError> {
    let mut arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.is_empty() {
        arguments.push("desktop".to_owned());
    }
    let requested_help = arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"));
    let version = crate::version();
    let config = figue::builder::<Cli>()
        .map_err(|_| ParseError::new("invalid CLI schema"))?
        .cli(|cli| cli.args(arguments).strict())
        .help(|help| help.program_name("poche").version(version))
        .build();
    match Driver::new(config).run().into_result() {
        Ok(output) => {
            let cli = output.get();
            cli.command.validate()?;
            Ok(ParseOutcome::Run(Box::new(cli)))
        }
        Err(DriverError::Help { text, .. }) if requested_help => {
            Ok(ParseOutcome::Write(format!("{}\n", strip_ansi(&text))))
        }
        Err(DriverError::Version { text }) => {
            Ok(ParseOutcome::Write(format!("{}\n", strip_ansi(&text))))
        }
        Err(DriverError::Completions { script }) => {
            Ok(ParseOutcome::Write(format!("{}\n", strip_ansi(&script))))
        }
        Err(_) => Err(ParseError::new("invalid command line; use --help")),
    }
}

fn strip_ansi(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| text.to_owned())
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
            (&["desktop"], ("desktop", "launch")),
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
            (
                &["game", "play-card", "r", "jack-spades"],
                ("game", "play-card"),
            ),
            (
                &["command", "parse", "/score add player1 100"],
                ("command", "parse"),
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
            (
                &["agent", "run", "alice-cli", "r", "legal-random"],
                ("agent", "run"),
            ),
            (&["device", "list"], ("device", "list")),
            (
                &[
                    "device",
                    "capture",
                    "request",
                    "alice-cli",
                    "r",
                    "device-renderer",
                    "bidding",
                ],
                ("device", "capture"),
            ),
            (&["puppet", "run", "full-round"], ("puppet", "run")),
            (&["puppet", "list"], ("puppet", "list")),
            (
                &["puppet", "show", "two-player-full-round"],
                ("puppet", "show"),
            ),
            (&["puppet", "artifacts", "path"], ("puppet", "artifacts")),
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
    fn no_arguments_means_desktop_and_help_is_generated_from_the_schema() {
        let ParseOutcome::Run(cli) = parse_args(Vec::<String>::new()).unwrap() else {
            panic!("expected default graphical command");
        };
        assert_eq!(cli.command.name(), ("desktop", "launch"));
        let ParseOutcome::Write(help) = parse_args(["--help".to_owned()]).unwrap() else {
            panic!("expected generated help");
        };
        for command in ["desktop", "device", "agent", "puppet", "transcript"] {
            assert!(help.contains(command), "help omitted {command}");
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
            "device",
            "capture",
            "puppet",
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

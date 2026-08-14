use facet::Facet;
use figue as args;
use poche_protocol::{CommandPayload, PrincipalId};

use crate::cli::{live_device::LiveDeviceConfig, output::OutputFormat};

#[derive(Facet, PartialEq, Eq)]
pub struct SpectatorArgs {
    #[facet(args::subcommand)]
    pub command: SpectatorCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum SpectatorCommand {
    RequestHand {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        player: String,
    },
    GrantHand {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        spectator: String,
    },
    RevokeHand {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        spectator: String,
    },
}

impl SpectatorArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            SpectatorCommand::RequestHand { .. } => "request-hand",
            SpectatorCommand::GrantHand { .. } => "grant-hand",
            SpectatorCommand::RevokeHand { .. } => "revoke-hand",
        }
    }

    /// Execute a hand-visibility request or grant transition through the
    /// selected certified device and its current advertised action set.
    ///
    /// # Errors
    ///
    /// Fails for invalid principal text, unavailable or ambiguous controls,
    /// transport/authorization errors, or output failures.
    pub fn invoke(self, config: &LiveDeviceConfig, output: OutputFormat) -> eyre::Result<bool> {
        match self.command {
            SpectatorCommand::RequestHand { room, player } => {
                let player = parse_principal(player)?;
                config.invoke_payload(
                    &room,
                    1,
                    &CommandPayload::RequestHand { player },
                    "spectator-request-hand",
                    output,
                )
            }
            SpectatorCommand::GrantHand { room, spectator } => {
                let spectator = parse_principal(spectator)?;
                config.invoke_matching_payload(
                    &room,
                    1,
                    "spectator-grant-hand",
                    "a hand grant for that spectator",
                    output,
                    |payload| {
                        matches!(
                            payload,
                            CommandPayload::GrantHand { recipient, .. } if recipient == &spectator
                        )
                    },
                )
            }
            SpectatorCommand::RevokeHand { room, spectator } => {
                let spectator = parse_principal(spectator)?;
                config.invoke_matching_payload(
                    &room,
                    1,
                    "spectator-revoke-hand",
                    "a hand grant for that spectator",
                    output,
                    |payload| {
                        matches!(
                            payload,
                            CommandPayload::RevokeHand { recipient, .. } if recipient == &spectator
                        )
                    },
                )
            }
        }
    }
}

fn parse_principal(value: String) -> eyre::Result<PrincipalId> {
    PrincipalId::new(value).map_err(|_| eyre::eyre!("player identifier is invalid"))
}

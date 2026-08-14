use facet::Facet;
use figue as args;

/// Room lifecycle command group.
#[derive(Facet, PartialEq, Eq)]
pub struct RoomArgs {
    #[facet(args::subcommand)]
    pub command: RoomCommand,
}

/// Room lifecycle commands. Invite material is sensitive and is never exposed
/// by command naming, Debug output, receipts, or parse errors.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum RoomCommand {
    Host {
        #[facet(args::positional)]
        room: String,
    },
    Join {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional, sensitive)]
        invite_code: String,
    },
    Show {
        #[facet(args::positional)]
        room: String,
    },
    Ready {
        #[facet(args::positional)]
        room: String,
    },
    Unready {
        #[facet(args::positional)]
        room: String,
    },
    Countdown {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        ticks: u64,
    },
    Abort {
        #[facet(args::positional)]
        room: String,
    },
    Pause {
        #[facet(args::positional)]
        room: String,
    },
    Resume {
        #[facet(args::positional)]
        room: String,
    },
    Leave {
        #[facet(args::positional)]
        room: String,
    },
    Close {
        #[facet(args::positional)]
        room: String,
    },
}

impl RoomArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            RoomCommand::Host { .. } => "host",
            RoomCommand::Join { .. } => "join",
            RoomCommand::Show { .. } => "show",
            RoomCommand::Ready { .. } => "ready",
            RoomCommand::Unready { .. } => "unready",
            RoomCommand::Countdown { .. } => "countdown",
            RoomCommand::Abort { .. } => "abort",
            RoomCommand::Pause { .. } => "pause",
            RoomCommand::Resume { .. } => "resume",
            RoomCommand::Leave { .. } => "leave",
            RoomCommand::Close { .. } => "close",
        }
    }

    /// Execute room commands supported by the certified external device
    /// transport. Join binds its bearer proof into the signed discovery read
    /// before the ordinary advertised action may be invoked.
    ///
    /// # Errors
    ///
    /// Returns a redacted profile, transport, observation, action, or output
    /// error without emitting invite or key material.
    pub fn invoke(
        self,
        config: &crate::cli::live_device::LiveDeviceConfig,
        output: crate::cli::output::OutputFormat,
    ) -> eyre::Result<bool> {
        use poche_protocol::CommandPayload;

        match self.command {
            RoomCommand::Host { room } => {
                config.invoke_payload(&room, 0, &CommandPayload::CreateRoom, "room-host", output)
            }
            RoomCommand::Join { room, invite_code } => {
                config.invoke_join(&room, &invite_code, output)
            }
            RoomCommand::Show { room } => config.observe(&room, 1, output),
            RoomCommand::Ready { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::Ready, "room-ready", output)
            }
            RoomCommand::Unready { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::Unready, "room-unready", output)
            }
            RoomCommand::Countdown { room, ticks } => config.invoke_countdown(&room, ticks, output),
            RoomCommand::Abort { room } => config.invoke_payload(
                &room,
                1,
                &CommandPayload::AbortCountdown,
                "room-abort",
                output,
            ),
            RoomCommand::Pause { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::Pause, "room-pause", output)
            }
            RoomCommand::Resume { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::Unpause, "room-resume", output)
            }
            RoomCommand::Leave { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::Leave, "room-leave", output)
            }
            RoomCommand::Close { room } => {
                config.invoke_payload(&room, 1, &CommandPayload::CloseRoom, "room-close", output)
            }
        }
    }
}

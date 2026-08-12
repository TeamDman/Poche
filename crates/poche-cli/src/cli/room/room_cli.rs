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
    Host,
    Join {
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
            RoomCommand::Host => "host",
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
}

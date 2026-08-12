use facet::Facet;
use figue as args;

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
}

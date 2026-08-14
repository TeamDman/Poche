use facet::Facet;
use figue as args;
use poche_protocol::CommandPayload;

use crate::cli::{live_device::LiveDeviceConfig, output::OutputFormat};

#[derive(Facet, PartialEq, Eq)]
pub struct ChatArgs {
    #[facet(args::subcommand)]
    pub command: ChatCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum ChatCommand {
    Send {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        message: String,
    },
    Tail {
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        limit: usize,
    },
}

impl ChatArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            ChatCommand::Send { .. } => "send",
            ChatCommand::Tail { .. } => "tail",
        }
    }

    /// Execute chat through the selected certified device's exact observation.
    ///
    /// # Errors
    ///
    /// Fails if the device cannot observe the room, chat is not currently
    /// advertised, the command is denied, or output cannot be written.
    pub fn invoke(self, config: &LiveDeviceConfig, output: OutputFormat) -> eyre::Result<bool> {
        match self.command {
            ChatCommand::Send { room, message } => config.invoke_payload(
                &room,
                1,
                &CommandPayload::Chat { text: message },
                "chat-send",
                output,
            ),
            ChatCommand::Tail { room, limit } => config.chat_tail(&room, limit, output),
        }
    }
}

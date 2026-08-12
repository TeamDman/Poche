use facet::Facet;
use figue as args;

#[derive(Facet, PartialEq, Eq)]
pub struct CaptureArgs {
    #[facet(args::subcommand)]
    pub command: CaptureCommand,
}

/// Cross-device capture cooperation commands. `target_device` is exact, not a
/// display label or a local process selector.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum CaptureCommand {
    Request {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        target_device: String,
        #[facet(args::positional)]
        label: String,
    },
    Cancel {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        request: String,
    },
}

impl CaptureArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            CaptureCommand::Request { .. } => "request",
            CaptureCommand::Cancel { .. } => "cancel",
        }
    }
}

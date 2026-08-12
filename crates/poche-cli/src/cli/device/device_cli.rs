use facet::Facet;
use figue as args;

use super::capture::CaptureArgs;

#[derive(Facet, PartialEq, Eq)]
pub struct DeviceArgs {
    #[facet(args::subcommand)]
    pub command: DeviceCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum DeviceCommand {
    List,
    Show {
        #[facet(args::positional)]
        profile: String,
    },
    Capture(CaptureArgs),
}

impl DeviceArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            DeviceCommand::List => "list",
            DeviceCommand::Show { .. } => "show",
            DeviceCommand::Capture(_) => "capture",
        }
    }
}

use facet::Facet;
use figue as args;
use poche_player_client::{DeviceProfile, ProtectedProfileStore};
use serde::Serialize;

use super::capture::CaptureArgs;
use crate::cli::{
    live_device::LiveDeviceConfig,
    output::{OutputFormat, emit_value},
};

#[derive(Facet, PartialEq, Eq)]
pub struct DeviceArgs {
    #[facet(args::subcommand)]
    pub command: DeviceCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum DeviceCommand {
    Create {
        #[facet(args::positional)]
        identity: String,
        #[facet(args::positional)]
        profile: String,
    },
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
            DeviceCommand::Create { .. } => "create",
            DeviceCommand::List => "list",
            DeviceCommand::Show { .. } => "show",
            DeviceCommand::Capture(_) => "capture",
        }
    }

    #[must_use]
    pub const fn reports_cancellation_as_result(&self) -> bool {
        match &self.command {
            DeviceCommand::Capture(arguments) => arguments.reports_cancellation_as_result(),
            DeviceCommand::Create { .. } | DeviceCommand::List | DeviceCommand::Show { .. } => {
                false
            }
        }
    }

    /// Execute protected device enrollment or public profile inspection.
    /// Capture subcommands remain owned by the cooperation execution path.
    ///
    /// # Errors
    ///
    /// Fails closed on unavailable/corrupt protected or public profile state.
    pub fn invoke(
        self,
        config: &LiveDeviceConfig,
        output: OutputFormat,
        cancelled: impl FnMut() -> bool,
    ) -> eyre::Result<bool> {
        if let DeviceCommand::Capture(arguments) = self.command {
            return arguments.invoke(config, output, cancelled);
        }
        let store = ProtectedProfileStore::open_default()?;
        let profiles = match self.command {
            DeviceCommand::Create { identity, profile } => {
                vec![store.create_device(&identity, &profile)?]
            }
            DeviceCommand::List => store.list_devices()?,
            DeviceCommand::Show { profile } => vec![store.load_device(&profile)?],
            DeviceCommand::Capture(_) => unreachable!("capture returned before profile dispatch"),
        };
        let summaries = profiles.iter().map(DeviceSummary::from).collect::<Vec<_>>();
        let text = if summaries.is_empty() {
            "No device profiles. Create one with `poche device create IDENTITY PROFILE`.".to_owned()
        } else {
            summaries
                .iter()
                .map(|profile| {
                    format!(
                        "{} — player {} — device {} — {:?} — sequence {}",
                        profile.label,
                        profile.player_id,
                        profile.device_id,
                        profile.capabilities,
                        profile.certificate_sequence
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        emit_value(&summaries, &text, output)?;
        Ok(true)
    }
}

#[derive(Serialize)]
struct DeviceSummary<'a> {
    schema: &'static str,
    label: &'a str,
    player_id: &'a str,
    device_id: &'a str,
    certificate_sequence: u64,
    custody: poche_protocol::DeviceCustodyWire,
    capabilities: &'a [poche_protocol::DeviceCapabilityWire],
    storage: &'static str,
}

impl<'a> From<&'a DeviceProfile> for DeviceSummary<'a> {
    fn from(profile: &'a DeviceProfile) -> Self {
        Self {
            schema: "poche.cli.device-profile.v1",
            label: &profile.label,
            player_id: profile.player_id.as_str(),
            device_id: profile.device_id.as_str(),
            certificate_sequence: profile.certificate.sequence,
            custody: profile.certificate.custody,
            capabilities: &profile.certificate.capabilities,
            storage: "os-credential-vault",
        }
    }
}

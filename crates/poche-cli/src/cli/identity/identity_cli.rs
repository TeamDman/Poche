use facet::Facet;
use figue as args;
use poche_player_client::{PlayerRootProfile, ProtectedProfileStore};
use serde::Serialize;

use crate::cli::output::{OutputFormat, emit_value};

#[derive(Facet, PartialEq, Eq)]
pub struct IdentityArgs {
    #[facet(args::subcommand)]
    pub command: IdentityCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum IdentityCommand {
    Show,
    Create {
        #[facet(args::positional)]
        label: String,
    },
}

impl IdentityArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            IdentityCommand::Show => "show",
            IdentityCommand::Create { .. } => "create",
        }
    }

    /// Execute protected root creation or public root inspection.
    ///
    /// # Errors
    ///
    /// Fails closed when the OS credential vault/public registry is
    /// unavailable or a profile is invalid. Secret bytes and handles are not
    /// emitted.
    pub fn invoke(self, output: OutputFormat) -> eyre::Result<bool> {
        let store = ProtectedProfileStore::open_default()?;
        let roots = match self.command {
            IdentityCommand::Create { label } => vec![store.create_player_root(&label)?],
            IdentityCommand::Show => store.list_player_roots()?,
        };
        let summaries = roots.iter().map(RootSummary::from).collect::<Vec<_>>();
        let text = if summaries.is_empty() {
            "No player identities. Create one with `poche identity create NAME`.".to_owned()
        } else {
            summaries
                .iter()
                .map(|root| format!("{} — {} — {}", root.label, root.player_id, root.storage))
                .collect::<Vec<_>>()
                .join("\n")
        };
        emit_value(&summaries, &text, output)?;
        Ok(true)
    }
}

#[derive(Serialize)]
struct RootSummary<'a> {
    schema: &'static str,
    label: &'a str,
    player_id: &'a str,
    storage: &'static str,
}

impl<'a> From<&'a PlayerRootProfile> for RootSummary<'a> {
    fn from(profile: &'a PlayerRootProfile) -> Self {
        Self {
            schema: "poche.cli.player-root.v1",
            label: &profile.label,
            player_id: profile.root.player_id.as_str(),
            storage: "os-credential-vault",
        }
    }
}

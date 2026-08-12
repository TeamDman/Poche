use facet::Facet;
use figue as args;

#[derive(Facet, PartialEq, Eq)]
pub struct AgentArgs {
    #[facet(args::subcommand)]
    pub command: AgentCommand,
}

/// Persistent policy devices. The policy chooses only from advertised actions.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum AgentCommand {
    Run {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        policy: String,
    },
}

impl AgentArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "run"
    }
}

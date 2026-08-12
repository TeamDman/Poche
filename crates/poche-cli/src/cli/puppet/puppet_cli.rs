use facet::Facet;
use figue as args;

#[derive(Facet, PartialEq, Eq)]
pub struct PuppetArgs {
    #[facet(args::subcommand)]
    pub command: PuppetCommand,
}

/// One harness invocation that fans out into distinct certified test devices.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum PuppetCommand {
    Run {
        #[facet(args::positional)]
        scenario: String,
        #[facet(args::named)]
        output_dir: Option<String>,
    },
}

impl PuppetArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        "run"
    }
}

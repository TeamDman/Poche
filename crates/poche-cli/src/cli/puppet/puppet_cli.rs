use std::path::PathBuf;

use eyre::{Context, Result};
use facet::Facet;
use figue as args;
use poche_puppet::{
    PuppetRunOptions, PuppetSurface, PuppetTransport, default_artifact_root, run_with_cancel,
    scenario, scenarios,
};
use serde::Serialize;

use crate::cli::{
    ParseError,
    output::{OutputFormat, write_stdout},
};

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
    List,
    Show {
        #[facet(args::positional)]
        scenario: String,
    },
    Run {
        #[facet(args::positional)]
        scenario: String,
        #[facet(args::named)]
        surface: Option<String>,
        #[facet(args::named)]
        transport: Option<String>,
        #[facet(args::named)]
        seed: Option<u64>,
        #[facet(args::named)]
        max_steps: Option<u32>,
        #[facet(args::named)]
        output_dir: Option<String>,
    },
    Artifacts(PuppetArtifactsArgs),
}

#[derive(Facet, PartialEq, Eq)]
pub struct PuppetArtifactsArgs {
    #[facet(args::subcommand)]
    pub command: PuppetArtifactsCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum PuppetArtifactsCommand {
    Path,
}

impl PuppetArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            PuppetCommand::List => "list",
            PuppetCommand::Show { .. } => "show",
            PuppetCommand::Run { .. } => "run",
            PuppetCommand::Artifacts(_) => "artifacts",
        }
    }

    pub(crate) fn validate(&self) -> std::result::Result<(), ParseError> {
        match &self.command {
            PuppetCommand::Run { surface, .. }
                if surface
                    .as_deref()
                    .is_some_and(|surface| surface != "headless") =>
            {
                Err(ParseError::new("--surface currently requires headless"))
            }
            PuppetCommand::Run { transport, .. }
                if transport.as_deref().is_some_and(|transport| {
                    !matches!(transport, "loopback-typed" | "loopback-ndjson")
                }) =>
            {
                Err(ParseError::new(
                    "--transport requires loopback-typed or loopback-ndjson",
                ))
            }
            PuppetCommand::Run {
                max_steps: Some(0), ..
            } => Err(ParseError::new("--max-steps must be greater than zero")),
            PuppetCommand::List
            | PuppetCommand::Show { .. }
            | PuppetCommand::Run { .. }
            | PuppetCommand::Artifacts(_) => Ok(()),
        }
    }

    /// Execute the semantic puppet catalog or one bounded harness run.
    ///
    /// # Errors
    ///
    /// Returns an unknown-scenario, device, cancellation, evidence, encoding,
    /// or stdout failure.
    pub fn invoke(self, format: OutputFormat, cancelled: impl FnMut() -> bool) -> Result<bool> {
        match self.command {
            PuppetCommand::List => {
                emit_serializable(
                    scenarios(),
                    &scenarios()
                        .iter()
                        .map(|scenario| format!("{} — {}", scenario.name, scenario.description))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    format,
                )?;
            }
            PuppetCommand::Show { scenario: name } => {
                let descriptor = scenario(&name)
                    .ok_or_else(|| eyre::eyre!("puppet scenario is not in the static catalog"))?;
                emit_serializable(
                    descriptor,
                    &format!(
                        "scenario: {}\ndescription: {}\nsurfaces: {}\ntransports: {}\nplayer roots: {}\ndevices: {}\ncompletion: {}",
                        descriptor.name,
                        descriptor.description,
                        descriptor.surfaces.join(", "),
                        descriptor.transports.join(", "),
                        descriptor.player_roots,
                        descriptor.devices,
                        descriptor.completion
                    ),
                    format,
                )?;
            }
            PuppetCommand::Run {
                scenario,
                surface: _,
                transport,
                seed,
                max_steps,
                output_dir,
            } => {
                let mut options = PuppetRunOptions {
                    scenario,
                    surface: PuppetSurface::Headless,
                    transport: match transport.as_deref() {
                        Some("loopback-ndjson") => PuppetTransport::LoopbackNdjson,
                        None | Some("loopback-typed") => PuppetTransport::LoopbackTyped,
                        Some(_) => unreachable!("validated transport"),
                    },
                    seed: seed.unwrap_or(1),
                    ..PuppetRunOptions::default()
                };
                if let Some(max_steps) = max_steps {
                    options.max_steps = max_steps;
                }
                if let Some(output_dir) = output_dir {
                    options.artifact_root = PathBuf::from(output_dir);
                }
                let report =
                    run_with_cancel(&options, cancelled).map_err(|error| eyre::eyre!(error))?;
                let summary = PuppetRunSummary::from(&report);
                emit_serializable(
                    &summary,
                    &format!(
                        "scenario: {}\nstatus: {}\nsurface: {}\nsteps: {}\nfinal revision: {}\nfinal scores: {:?}\nartifacts: {}",
                        report.scenario,
                        report.status,
                        report.surface,
                        report.step_count,
                        report.final_revision,
                        report.final_scores,
                        report.artifact_directory
                    ),
                    format,
                )?;
            }
            PuppetCommand::Artifacts(PuppetArtifactsArgs {
                command: PuppetArtifactsCommand::Path,
            }) => {
                let path = ArtifactPathOutput {
                    schema: "poche.puppet.artifact-root.v1",
                    path: default_artifact_root().to_string_lossy().into_owned(),
                };
                emit_serializable(&path, &path.path, format)?;
            }
        }
        Ok(true)
    }
}

#[derive(Serialize)]
struct ArtifactPathOutput {
    schema: &'static str,
    path: String,
}

#[derive(Serialize)]
struct PuppetRunSummary<'a> {
    schema: &'static str,
    run_id: &'a str,
    scenario: &'a str,
    surface: &'a str,
    transport: &'a str,
    seed: u64,
    status: &'a str,
    final_revision: u64,
    final_room_phase: &'a str,
    final_scores: &'a [u16],
    step_count: u32,
    devices: usize,
    public_history_events: usize,
    public_history_hash: &'a str,
    artifact_directory: &'a str,
    evidence_boundary: &'a str,
}

impl<'a> From<&'a poche_puppet::PuppetRunReport> for PuppetRunSummary<'a> {
    fn from(report: &'a poche_puppet::PuppetRunReport) -> Self {
        Self {
            schema: "poche.puppet.run-summary.v1",
            run_id: &report.run_id,
            scenario: &report.scenario,
            surface: &report.surface,
            transport: &report.transport,
            seed: report.seed,
            status: &report.status,
            final_revision: report.final_revision,
            final_room_phase: &report.final_room_phase,
            final_scores: &report.final_scores,
            step_count: report.step_count,
            devices: report.devices.len(),
            public_history_events: report.public_history_events,
            public_history_hash: &report.public_history_hash,
            artifact_directory: &report.artifact_directory,
            evidence_boundary: &report.evidence_boundary,
        }
    }
}

fn emit_serializable(
    value: &(impl Serialize + ?Sized),
    text: &str,
    format: OutputFormat,
) -> Result<()> {
    match format {
        OutputFormat::Text => write_stdout(&format!("{text}\n")),
        OutputFormat::Json => write_stdout(&format!(
            "{}\n",
            serde_json::to_string_pretty(value).wrap_err("failed to encode puppet JSON")?
        )),
        OutputFormat::Ndjson => write_stdout(&format!(
            "{}\n",
            serde_json::to_string(value).wrap_err("failed to encode puppet NDJSON")?
        )),
    }
}

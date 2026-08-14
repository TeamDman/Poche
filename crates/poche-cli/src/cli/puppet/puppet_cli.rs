use std::path::PathBuf;

use eyre::{Context, Result};
use facet::Facet;
use figue as args;
use poche_puppet::{
    EXTERNAL_DEVICES_FULL_GAME, PuppetRunOptions, PuppetSurface, PuppetTransport,
    default_artifact_root, run_with_cancel, scenario, scenarios,
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
        /// Open the native puppet renderer for interactive debugging. Native
        /// automation is windowless unless this is explicitly supplied.
        #[facet(args::named, default)]
        show_window: bool,
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
    Open,
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
                    .is_some_and(|surface| !valid_surface_list(surface)) =>
            {
                Err(ParseError::new(
                    "--surface requires a comma-separated selection of headless, web, or native",
                ))
            }
            PuppetCommand::Run {
                surface,
                show_window: true,
                ..
            } if surface.as_deref() != Some("native") => {
                Err(ParseError::new("--show-window requires --surface native"))
            }
            PuppetCommand::Run { transport, .. }
                if transport.as_deref().is_some_and(|transport| {
                    !matches!(
                        transport,
                        "loopback-typed" | "loopback-ndjson" | "http-loopback"
                    )
                }) =>
            {
                Err(ParseError::new(
                    "--transport requires loopback-typed, loopback-ndjson, or http-loopback",
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
    pub fn invoke(self, format: OutputFormat, mut cancelled: impl FnMut() -> bool) -> Result<bool> {
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
                surface,
                transport,
                seed,
                max_steps,
                output_dir,
                show_window,
            } => {
                let surfaces = surface_list(surface.as_deref());
                let transport = match transport.as_deref() {
                    Some("loopback-ndjson") => PuppetTransport::LoopbackNdjson,
                    Some("http-loopback") => PuppetTransport::HttpLoopback,
                    None if scenario == EXTERNAL_DEVICES_FULL_GAME => PuppetTransport::HttpLoopback,
                    None | Some("loopback-typed") => PuppetTransport::LoopbackTyped,
                    Some(_) => unreachable!("validated transport"),
                };
                let mut options = PuppetRunOptions {
                    scenario,
                    surface: surfaces[0],
                    transport,
                    seed: seed.unwrap_or(1),
                    show_native_window: show_window,
                    ..PuppetRunOptions::default()
                };
                if let Some(max_steps) = max_steps {
                    options.max_steps = max_steps;
                }
                if let Some(output_dir) = output_dir {
                    options.artifact_root = PathBuf::from(output_dir);
                }
                let mut reports = Vec::with_capacity(surfaces.len());
                for surface in surfaces {
                    options.surface = surface;
                    reports.push(
                        run_with_cancel(&options, &mut cancelled)
                            .map_err(|error| eyre::eyre!(error))?,
                    );
                }
                if reports.len() == 1 {
                    emit_run(&reports[0], format)?;
                } else {
                    emit_suite(&reports, &options.artifact_root, format)?;
                }
            }
            PuppetCommand::Artifacts(arguments) => invoke_artifacts(&arguments.command, format)?,
        }
        Ok(true)
    }
}

fn valid_surface_list(value: &str) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    !value.is_empty()
        && value
            .split(',')
            .all(|surface| matches!(surface, "headless" | "native" | "web") && seen.insert(surface))
}

fn surface_list(value: Option<&str>) -> Vec<PuppetSurface> {
    value
        .unwrap_or("headless")
        .split(',')
        .map(|surface| match surface {
            "native" => PuppetSurface::Native,
            "web" => PuppetSurface::Web,
            "headless" => PuppetSurface::Headless,
            _ => unreachable!("validated surface"),
        })
        .collect()
}

fn emit_run(report: &poche_puppet::PuppetRunReport, format: OutputFormat) -> Result<()> {
    let summary = PuppetRunSummary::from(report);
    emit_serializable(
        &summary,
        &format!(
            "scenario: {}\nstatus: {}\nsurface: {}\nsteps: {}\nlifecycle transitions: {}\ncaptures: {}\nfinal revision: {}\nfinal scores: {:?}\nartifacts: {}",
            report.scenario,
            report.status,
            report.surface,
            report.step_count,
            report.lifecycle.len(),
            report.captures.len(),
            report.final_revision,
            report.final_scores,
            report.artifact_directory
        ),
        format,
    )
}

fn emit_suite(
    reports: &[poche_puppet::PuppetRunReport],
    artifact_root: &std::path::Path,
    format: OutputFormat,
) -> Result<()> {
    let summaries = reports
        .iter()
        .map(PuppetRunSummary::from)
        .collect::<Vec<_>>();
    let suite = PuppetRunSuiteSummary {
        schema: "poche.puppet.run-suite-summary.v1",
        status: "complete",
        surfaces: summaries,
        artifact_root: artifact_root.to_string_lossy().into_owned(),
        contact_sheet: artifact_root
            .join("index.html")
            .to_string_lossy()
            .into_owned(),
        catalog: artifact_root
            .join("catalog.json")
            .to_string_lossy()
            .into_owned(),
    };
    let text = format!(
        "status: complete\nsurfaces: {}\nartifacts: {}\ncontact sheet: {}",
        reports
            .iter()
            .map(|report| report.surface.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        suite.artifact_root,
        suite.contact_sheet
    );
    emit_serializable(&suite, &text, format)
}

fn invoke_artifacts(command: &PuppetArtifactsCommand, format: OutputFormat) -> Result<()> {
    match command {
        PuppetArtifactsCommand::Path => {
            let path = ArtifactPathOutput {
                schema: "poche.puppet.artifact-root.v1",
                path: default_artifact_root().to_string_lossy().into_owned(),
            };
            emit_serializable(&path, &path.path, format)
        }
        PuppetArtifactsCommand::Open => {
            let root = default_artifact_root();
            let target = if root.join("index.html").is_file() {
                root.join("index.html")
            } else {
                root
            };
            open_artifact_target(&target)?;
            let opened = ArtifactPathOutput {
                schema: "poche.puppet.artifact-open.v1",
                path: target.to_string_lossy().into_owned(),
            };
            emit_serializable(&opened, &opened.path, format)
        }
    }
}

fn open_artifact_target(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    command
        .spawn()
        .wrap_err("failed to open the puppet artifact catalog")?;
    Ok(())
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
    lifecycle_transitions: usize,
    captures: usize,
    public_history_events: usize,
    public_history_hash: &'a str,
    artifact_directory: &'a str,
    evidence_boundary: &'a str,
}

#[derive(Serialize)]
struct PuppetRunSuiteSummary<'a> {
    schema: &'static str,
    status: &'static str,
    surfaces: Vec<PuppetRunSummary<'a>>,
    artifact_root: String,
    contact_sheet: String,
    catalog: String,
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
            lifecycle_transitions: report.lifecycle.len(),
            captures: report.captures.len(),
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

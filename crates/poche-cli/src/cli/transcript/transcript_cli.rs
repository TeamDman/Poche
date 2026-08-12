use eyre::{Context, Result};
use facet::Facet;
use figue as args;
use poche_runtime::{
    GoldenTranscript, render_transcript_output_ndjson, render_transcript_text,
    replay_fixture_script_ndjson,
};
use serde::Serialize;

use super::super::output::{OutputFormat, write_stdout};

#[derive(Facet, PartialEq, Eq)]
pub struct TranscriptArgs {
    #[facet(args::subcommand)]
    pub command: TranscriptCommand,
}

#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum TranscriptCommand {
    Record {
        #[facet(args::positional)]
        path: String,
    },
    Replay {
        #[facet(args::positional)]
        path: String,
    },
    Inspect {
        #[facet(args::positional)]
        path: String,
    },
}

impl TranscriptArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            TranscriptCommand::Record { .. } => "record",
            TranscriptCommand::Replay { .. } => "replay",
            TranscriptCommand::Inspect { .. } => "inspect",
        }
    }

    /// Execute presentation/replay-only transcript commands.
    ///
    /// `record` remains a parsed runtime command until a live session recorder
    /// is attached; returning `false` lets the top level emit that receipt.
    ///
    /// # Errors
    ///
    /// Returns a file, replay, serialization, or stdout failure.
    pub fn invoke(self, format: OutputFormat) -> Result<bool> {
        match self.command {
            TranscriptCommand::Record { .. } => Ok(false),
            TranscriptCommand::Replay { path } => {
                let script = std::fs::read_to_string(&path)
                    .wrap_err_with(|| format!("failed to read transcript script {path}"))?;
                let replay = replay_fixture_script_ndjson(&script)
                    .map_err(|error| eyre::eyre!("transcript replay failed: {error}"))?;
                match format {
                    OutputFormat::Text => write_stdout(&replay.text)?,
                    OutputFormat::Json => write_stdout(&format!(
                        "{}\n",
                        serde_json::to_string(&ReplaySummary::from(&replay.transcript))
                            .wrap_err("failed to encode replay summary")?
                    ))?,
                    OutputFormat::Ndjson => write_stdout(&replay.output_ndjson)?,
                }
                Ok(true)
            }
            TranscriptCommand::Inspect { path } => {
                let transcript = std::fs::read_to_string(&path)
                    .wrap_err_with(|| format!("failed to read transcript {path}"))?;
                let transcript: GoldenTranscript = serde_json::from_str(&transcript)
                    .wrap_err("transcript inspect requires a GoldenTranscript JSON file")?;
                match format {
                    OutputFormat::Text => write_stdout(&render_transcript_text(&transcript))?,
                    OutputFormat::Json => write_stdout(&format!(
                        "{}\n",
                        serde_json::to_string(&ReplaySummary::from(&transcript))
                            .wrap_err("failed to encode transcript summary")?
                    ))?,
                    OutputFormat::Ndjson => write_stdout(
                        &render_transcript_output_ndjson(&transcript)
                            .map_err(|error| eyre::eyre!(error))?,
                    )?,
                }
                Ok(true)
            }
        }
    }
}

#[derive(Serialize)]
struct ReplaySummary<'a> {
    schema: &'static str,
    fixture_id: &'a str,
    steps: usize,
    final_state_hash: &'a str,
}

impl<'a> From<&'a GoldenTranscript> for ReplaySummary<'a> {
    fn from(transcript: &'a GoldenTranscript) -> Self {
        Self {
            schema: "poche.transcript.replay-summary.v1",
            fixture_id: &transcript.fixture_id,
            steps: transcript.steps.len(),
            final_state_hash: &transcript.final_state_hash,
        }
    }
}

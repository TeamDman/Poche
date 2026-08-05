use eyre::{Context, Result};
use poche_runtime::{
    GoldenTranscript, render_transcript_output_ndjson, render_transcript_text,
    replay_fixture_script_ndjson,
};
use serde::Serialize;

use super::super::output::{OutputFormat, write_stdout};
use super::super::{ParseError, exact};

#[derive(Debug, PartialEq, Eq)]
pub enum TranscriptArgs {
    Record { path: String },
    Replay { path: String },
    Inspect { path: String },
}

impl TranscriptArgs {
    pub(crate) fn parse(arguments: &[String]) -> Result<Self, ParseError> {
        let (command, arguments) = arguments.split_first().ok_or_else(|| {
            ParseError::new("transcript command is required; use transcript --help")
        })?;
        let path = exact(arguments, 1)?[0].clone();
        match command.as_str() {
            "record" => Ok(Self::Record { path }),
            "replay" => Ok(Self::Replay { path }),
            "inspect" => Ok(Self::Inspect { path }),
            _ => Err(ParseError::new(
                "unknown transcript command; use transcript --help",
            )),
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Record { .. } => "record",
            Self::Replay { .. } => "replay",
            Self::Inspect { .. } => "inspect",
        }
    }

    /// Execute presentation/replay-only transcript commands.
    ///
    /// `record` remains a parsed runtime command until a live session recorder
    /// is attached; returning `false` lets the top level emit that receipt.
    ///
    /// # Errors
    ///
    /// Returns a file, script, serialization, or stdout failure.
    pub fn invoke(self, format: OutputFormat) -> Result<bool> {
        match self {
            Self::Record { .. } => Ok(false),
            Self::Replay { path } => {
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
            Self::Inspect { path } => {
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

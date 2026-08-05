use core::str::FromStr;
use std::io::Write;

use eyre::{Context, Result};
use serde::Serialize;

use super::ParseError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Ndjson,
}

impl FromStr for OutputFormat {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "ndjson" => Ok(Self::Ndjson),
            _ => Err(ParseError::new("--output requires text, json, or ndjson")),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommandReceipt {
    schema: &'static str,
    command: String,
    status: &'static str,
    message: &'static str,
}

impl CommandReceipt {
    #[must_use]
    pub fn parsed(group: &str, action: &str) -> Self {
        Self {
            schema: "poche.cli.command-receipt.v1",
            command: format!("{group}.{action}"),
            status: "parsed",
            message: "execution is supplied by the selected runtime transport",
        }
    }

    fn text(&self) -> String {
        format!(
            "schema: {}\ncommand: {}\nstatus: {}\nmessage: {}\n",
            self.schema, self.command, self.status, self.message
        )
    }
}

/// Render a typed receipt to stdout.
///
/// # Errors
///
/// Returns an error if JSON encoding or stdout writing fails.
pub fn emit(receipt: &CommandReceipt, format: OutputFormat) -> Result<()> {
    let rendered = match format {
        OutputFormat::Text => receipt.text(),
        OutputFormat::Json => format!(
            "{}\n",
            serde_json::to_string(receipt).wrap_err("failed to encode JSON output")?
        ),
        OutputFormat::Ndjson => format!(
            "{}\n",
            serde_json::to_string(receipt).wrap_err("failed to encode NDJSON output")?
        ),
    };
    write_stdout(&rendered)
}

/// Write a pre-rendered help/version response to stdout.
///
/// # Errors
///
/// Returns an error if writing or flushing stdout fails.
pub fn write_stdout(output: &str) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(output.as_bytes())
        .wrap_err("failed to write stdout")?;
    stdout.flush().wrap_err("failed to flush stdout")
}

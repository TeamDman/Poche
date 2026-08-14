//! Inspectable command-line boundary for Poche clients and automation.

pub mod cli;
mod logging;

use std::ffi::OsString;

use cli::{ParseOutcome, parse_args};
use eyre::{Context, Result};
use teamy_cancellation::CtrlCHandler;
use tracing::info;

/// Run using process arguments and standard streams.
///
/// # Errors
///
/// Returns an error when arguments, logging, cancellation, or output fail.
pub fn run() -> Result<()> {
    run_from(std::env::args_os().skip(1))
}

/// Run using a supplied argument sequence.
///
/// This seam keeps argument parsing directly testable and does not let parser
/// errors terminate the process from inside a library.
///
/// # Errors
///
/// Returns an error when arguments, logging, cancellation, or output fail.
pub fn run_from(arguments: impl IntoIterator<Item = OsString>) -> Result<()> {
    let arguments = arguments
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| eyre::eyre!("argument is not valid Unicode"))
        })
        .collect::<Result<Vec<_>>>()?;

    match parse_args(arguments).map_err(|error| eyre::eyre!(error))? {
        ParseOutcome::Write(output) => cli::output::write_stdout(&output),
        ParseOutcome::Run(parsed) => {
            let cancellation = CtrlCHandler::default()
                .install()
                .wrap_err("failed to install Ctrl+C handler")?;
            let _stop_after = parsed.global.stop_after_ms.and_then(|milliseconds| {
                if milliseconds == 0 {
                    cancellation.request_cancel("--stop-after-ms elapsed");
                    None
                } else {
                    let cancellation = cancellation.clone();
                    Some(std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
                        cancellation.request_cancel("--stop-after-ms elapsed");
                    }))
                }
            });
            let _logging_guard = logging::init(&parsed.global)?;
            cancellation.bail_if_cancelled()?;
            let (group, action) = parsed.command.name();
            info!(
                command_group = group,
                command_action = action,
                "command parsed"
            );
            let live_config = cli::live_device::LiveDeviceConfig::from_global(&parsed.global);
            let emitted = match parsed.command {
                cli::Command::Desktop(command) => command.invoke()?,
                cli::Command::Room(command) => {
                    command.invoke(&live_config, parsed.global.output)?
                }
                cli::Command::Game(command) => {
                    command.invoke(&live_config, parsed.global.output)?
                }
                cli::Command::Transcript(command) => command.invoke(parsed.global.output)?,
                cli::Command::Governance(command) => command.invoke(parsed.global.output)?,
                cli::Command::Identity(command) => command.invoke(parsed.global.output)?,
                cli::Command::Device(command) => command.invoke(parsed.global.output)?,
                cli::Command::Puppet(command) => command.invoke(parsed.global.output, || {
                    cancellation.bail_if_cancelled().is_err()
                })?,
                _ => false,
            };
            if !emitted {
                let output = cli::output::CommandReceipt::parsed(group, action);
                cli::output::emit(&output, parsed.global.output)?;
            }
            cancellation.bail_if_cancelled()?;
            Ok(())
        }
    }
}

/// Package and source revision metadata embedded by the crate build script.
#[must_use]
pub fn version() -> String {
    format!(
        "{} (repo {}, branch {}, rev {}, worktree {}, built-unix {})",
        env!("CARGO_PKG_VERSION"),
        env!("POCHE_GIT_REPOSITORY"),
        env!("POCHE_GIT_BRANCH"),
        env!("POCHE_GIT_REVISION"),
        env!("POCHE_GIT_WORKTREE"),
        env!("POCHE_BUILD_UNIX")
    )
}

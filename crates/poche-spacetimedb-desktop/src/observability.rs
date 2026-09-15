// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Durable desktop logs and crash evidence.

use bevy::{log::BoxedLayer, prelude::App};
use std::{
    backtrace::Backtrace,
    fs::{File, OpenOptions},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing_subscriber::fmt::MakeWriter;

static LOG_WRITER: OnceLock<SharedLogWriter> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone)]
struct SharedLogWriter(Arc<Mutex<File>>);

struct SharedLogGuard<'a>(MutexGuard<'a, File>);

impl Write for SharedLogGuard<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<'a> MakeWriter<'a> for SharedLogWriter {
    type Writer = SharedLogGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        SharedLogGuard(self.0.lock().expect("Poche log mutex poisoned"))
    }
}

/// Prepare the file writer consumed by Bevy's logging plugin and install the panic hook.
///
/// # Errors
///
/// Returns an error when the application-data directory or requested log destination cannot be
/// created or opened.
pub fn initialize(requested: Option<&Path>) -> Result<PathBuf, String> {
    let path = resolve_log_path(requested)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create Poche log directory: {error}"))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("could not open Poche log file: {error}"))?;
    LOG_WRITER
        .set(SharedLogWriter(Arc::new(Mutex::new(file))))
        .map_err(|_| "Poche logging was initialized more than once".to_string())?;
    LOG_PATH
        .set(path.clone())
        .map_err(|_| "Poche log path was initialized more than once".to_string())?;
    install_panic_hook(path.clone());
    Ok(path)
}

/// Bevy `LogPlugin` hook that adds the durable file layer without replacing terminal output.
pub(crate) fn file_log_layer(_app: &mut App) -> Option<BoxedLayer> {
    let writer = LOG_WRITER.get()?.clone();
    Some(Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_file(true)
            .with_line_number(true)
            .with_target(true)
            .with_writer(writer),
    ))
}

/// Wait for acknowledgement after a fatal interactive error so an auto-closing terminal remains
/// readable. Redirected and automated processes never wait.
pub fn pause_console_on_failure() {
    if std::env::var_os("POCHE_NO_CRASH_PAUSE").is_some()
        || !io::stdin().is_terminal()
        || !io::stderr().is_terminal()
    {
        return;
    }
    eprintln!("Press Enter to close Poche.");
    let mut acknowledgement = String::new();
    let _ = io::stdin().read_line(&mut acknowledgement);
}

fn resolve_log_path(requested: Option<&Path>) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_millis();
    let filename = format!("poche-{timestamp}-{}.log", std::process::id());
    if let Some(requested) = requested {
        return Ok(if requested.is_dir() {
            requested.join(filename)
        } else {
            requested.to_path_buf()
        });
    }
    let root = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .ok_or("LOCALAPPDATA or APPDATA is required unless --log-file is set")?;
    Ok(root.join("Poche").join("logs").join(filename))
}

fn install_panic_hook(path: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "\nPOCHE CRASH");
            let _ = writeln!(file, "panic: {panic_info}");
            let _ = writeln!(file, "backtrace:\n{}", Backtrace::force_capture());
            let _ = file.flush();
        }
        eprintln!("Poche crashed. Crash log: {}", path.display());
        previous(panic_info);
        pause_console_on_failure();
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_file_path_is_preserved() {
        let path = Path::new("logs/poche-test.log");
        assert_eq!(resolve_log_path(Some(path)).expect("log path"), path);
    }

    #[test]
    fn existing_directory_receives_unique_log_name() {
        let directory = tempfile::tempdir().expect("temporary log directory");
        let path = resolve_log_path(Some(directory.path())).expect("directory log path");
        assert_eq!(path.parent(), Some(directory.path()));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("poche-")
        );
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("log")
        );
    }
}

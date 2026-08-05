use std::fs::File;
use std::sync::{Arc, Mutex};

use eyre::{Context, Result, bail};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cli::GlobalArgs;

pub(crate) struct LoggingGuard;

pub(crate) fn init(global: &GlobalArgs) -> Result<LoggingGuard> {
    if global.debug && global.log_filter.is_some() {
        bail!("--debug and --log-filter cannot be used together");
    }
    let filter = global.log_filter.clone().unwrap_or_else(|| {
        if global.debug {
            "warn,poche_cli=debug".to_owned()
        } else {
            "warn,poche_cli=info".to_owned()
        }
    });

    let stderr_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(false)
        .with_target(true)
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::builder().parse(&filter)?);

    let ndjson_layer = if let Some(path) = &global.log_file {
        let path = std::path::Path::new(path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("failed to create log directory {}", parent.display()))?;
        }
        let file = Arc::new(Mutex::new(File::create(path).wrap_err_with(|| {
            format!("failed to create NDJSON log {}", path.display())
        })?));
        let writer = BoxMakeWriter::new(move || {
            file.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .try_clone()
                .expect("cloning an open log file should succeed")
        });
        Some(
            tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_writer(writer)
                .with_filter(EnvFilter::builder().parse(&filter)?),
        )
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(ndjson_layer)
        .try_init()
        .wrap_err("failed to initialize logging")?;
    Ok(LoggingGuard)
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use poche_capture::{CapturePipeline, PersistedCapture};

use crate::{
    PuppetError, PuppetErrorCode, PuppetRunOptions,
    scenario::{PuppetExecution, PuppetRunReport},
};

#[derive(Serialize, Deserialize)]
struct PuppetArtifactManifest {
    schema: String,
    run_id: String,
    scenario: String,
    surface: String,
    status: String,
    qualification: String,
    files: Vec<PuppetArtifactFile>,
}

#[derive(Serialize, Deserialize)]
struct PuppetArtifactFile {
    path: String,
    media_type: String,
    bytes: u64,
    blake3: String,
}

struct TemporaryRunDirectory {
    path: PathBuf,
    published: bool,
}

impl Drop for TemporaryRunDirectory {
    fn drop(&mut self) {
        if !self.published {
            let _ignored = fs::remove_dir_all(&self.path);
        }
    }
}

pub(crate) fn persist_run(
    options: &PuppetRunOptions,
    execution: PuppetExecution,
) -> Result<PuppetRunReport, PuppetError> {
    let PuppetExecution {
        mut report,
        captures,
    } = execution;
    fs::create_dir_all(&options.artifact_root).map_err(evidence_error)?;
    let final_path = unique_run_path(&options.artifact_root, &report.run_id)?;
    let temporary_path = unique_temporary_path(&options.artifact_root, &report.run_id)?;
    fs::create_dir(&temporary_path).map_err(evidence_error)?;
    let mut temporary = TemporaryRunDirectory {
        path: temporary_path.clone(),
        published: false,
    };
    report.artifact_directory = final_path.to_string_lossy().into_owned();

    report.captures.clear();
    let mut capture_files = Vec::new();
    for mut capture in captures {
        let persisted = CapturePipeline::new(temporary_path.join("captures"))
            .persist(&capture.bundle)
            .map_err(capture_error)?;
        let relative_directory = persisted
            .directory
            .strip_prefix(&temporary_path)
            .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
        let relative_manifest = persisted
            .manifest_path
            .strip_prefix(&temporary_path)
            .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
        capture.evidence.artifact_directory = final_path
            .join(relative_directory)
            .to_string_lossy()
            .into_owned();
        capture.evidence.manifest_path = final_path
            .join(relative_manifest)
            .to_string_lossy()
            .into_owned();
        capture_files.extend(describe_capture_files(&temporary_path, &persisted)?);
        report.captures.push(capture.evidence);
    }

    let report_path = temporary_path.join("run.json");
    write_json(&report_path, &report)?;
    let steps_path = temporary_path.join("steps.ndjson");
    write_steps(&steps_path, &report)?;
    let mut files = vec![
        describe_file(&report_path, "run.json", "application/json")?,
        describe_file(&steps_path, "steps.ndjson", "application/x-ndjson")?,
    ];
    files.extend(capture_files);
    let manifest = PuppetArtifactManifest {
        schema: "poche.puppet.artifact-manifest.v1".to_owned(),
        run_id: report.run_id.clone(),
        scenario: report.scenario.clone(),
        surface: report.surface.clone(),
        status: report.status.clone(),
        qualification: if report.captures.is_empty() {
            "Headless semantic evidence only; no graphical-capture claim.".to_owned()
        } else {
            "Certified full-game semantic evidence plus authorized semantic-checkpoint native render-target captures transferred privately and persisted by the requester; no main-menu/reconnect or external-network claim.".to_owned()
        },
        files,
    };
    write_json(&temporary_path.join("manifest.json"), &manifest)?;
    fs::rename(&temporary_path, &final_path).map_err(evidence_error)?;
    temporary.published = true;
    Ok(report)
}

fn describe_capture_files(
    run_root: &Path,
    persisted: &PersistedCapture,
) -> Result<Vec<PuppetArtifactFile>, PuppetError> {
    let mut files = Vec::with_capacity(persisted.manifest.entries.len() + 1);
    let manifest_relative = persisted
        .manifest_path
        .strip_prefix(run_root)
        .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
    files.push(describe_file(
        &persisted.manifest_path,
        &portable_relative(manifest_relative),
        "application/json",
    )?);
    for entry in &persisted.manifest.entries {
        let path = persisted.directory.join(&entry.relative_path);
        let relative = path
            .strip_prefix(run_root)
            .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
        files.push(describe_file(
            &path,
            &portable_relative(relative),
            &entry.media_type,
        )?);
    }
    Ok(files)
}

fn portable_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unique_run_path(root: &Path, run_id: &str) -> Result<PathBuf, PuppetError> {
    for suffix in 0_u16..=u16::MAX {
        let name = if suffix == 0 {
            run_id.to_owned()
        } else {
            format!("{run_id}-{suffix}")
        };
        let candidate = root.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(PuppetError::new(
        PuppetErrorCode::EvidenceIo,
        "no unique puppet artifact directory is available",
    ))
}

fn unique_temporary_path(root: &Path, run_id: &str) -> Result<PathBuf, PuppetError> {
    let process = std::process::id();
    for suffix in 0_u16..=u16::MAX {
        let candidate = root.join(format!(".{run_id}.tmp-{process}-{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(PuppetError::new(
        PuppetErrorCode::EvidenceIo,
        "no temporary puppet artifact directory is available",
    ))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), PuppetError> {
    let file = create_new(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value).map_err(|_| {
        PuppetError::new(
            PuppetErrorCode::EvidenceIo,
            "puppet JSON evidence could not be encoded",
        )
    })?;
    writer.write_all(b"\n").map_err(evidence_error)?;
    writer.flush().map_err(evidence_error)
}

fn write_steps(path: &Path, report: &PuppetRunReport) -> Result<(), PuppetError> {
    let file = create_new(path)?;
    let mut writer = BufWriter::new(file);
    for step in &report.steps {
        serde_json::to_writer(&mut writer, step).map_err(|_| {
            PuppetError::new(
                PuppetErrorCode::EvidenceIo,
                "puppet NDJSON evidence could not be encoded",
            )
        })?;
        writer.write_all(b"\n").map_err(evidence_error)?;
    }
    writer.flush().map_err(evidence_error)
}

fn create_new(path: &Path) -> Result<File, PuppetError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(evidence_error)
}

fn describe_file(
    path: &Path,
    relative: &str,
    media_type: &str,
) -> Result<PuppetArtifactFile, PuppetError> {
    let bytes = fs::read(path).map_err(evidence_error)?;
    Ok(PuppetArtifactFile {
        path: relative.to_owned(),
        media_type: media_type.to_owned(),
        bytes: u64::try_from(bytes.len()).map_err(|_| {
            PuppetError::new(
                PuppetErrorCode::EvidenceIo,
                "puppet artifact length overflowed",
            )
        })?,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn evidence_error(_: std::io::Error) -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::EvidenceIo,
        "puppet evidence filesystem operation failed",
    )
}

fn capture_error(_: poche_capture::CapturePipelineError) -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::EvidenceIo,
        "puppet capture artifact pipeline failed",
    )
}

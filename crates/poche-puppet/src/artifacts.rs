// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use poche_capture::{CapturePipeline, PersistedCapture};

use crate::{
    PuppetError, PuppetErrorCode, PuppetRunOptions,
    scenario::{PuppetCaptureEvidence, PuppetExecution, PuppetRunReport},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PuppetArtifactManifest {
    schema: String,
    executable_revision: String,
    executable_worktree: String,
    run_id: String,
    scenario: String,
    surface: String,
    status: String,
    final_revision: u64,
    qualification: String,
    evidence_boundary: String,
    public_history_hash: String,
    contact_sheet: String,
    captures: Vec<PuppetArtifactCapture>,
    files: Vec<PuppetArtifactFile>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PuppetArtifactCapture {
    label: String,
    request_id: String,
    requester_device_id: String,
    provider_device_id: String,
    requested_revision: u64,
    captured_revision: u64,
    projection_hash: String,
    scene_hash: Option<String>,
    provider_kind: String,
    representations: String,
    qualification: String,
    viewport_width: u32,
    viewport_height: u32,
    framebuffer_width: u32,
    framebuffer_height: u32,
    scale_milli: u32,
    camera_present: bool,
    windowless: bool,
    transferred_bytes: u64,
    transfer_chunks: u32,
    manifest_path: String,
    preview_path: Option<String>,
    structural_paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PuppetArtifactFile {
    path: String,
    media_type: String,
    bytes: u64,
    blake3: String,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PuppetArtifactCatalog {
    schema: String,
    generated_from_verified_manifests: bool,
    contact_sheet: String,
    contact_sheet_blake3: String,
    runs: Vec<PuppetArtifactCatalogRun>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PuppetArtifactCatalogRun {
    run_id: String,
    directory: String,
    scenario: String,
    surface: String,
    status: String,
    final_revision: u64,
    executable_revision: String,
    public_history_hash: String,
    contact_sheet: String,
    captures: Vec<PuppetArtifactCapture>,
    evidence_boundary: String,
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
    let mut capture_index = Vec::new();
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
        capture_index.push(index_capture(
            &temporary_path,
            &persisted,
            &capture.evidence,
        )?);
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
    let contact_sheet_path = temporary_path.join("index.html");
    write_bytes(
        &contact_sheet_path,
        &render_run_contact_sheet(&report, &capture_index),
    )?;
    files.push(describe_file(
        &contact_sheet_path,
        "index.html",
        "text/html; charset=utf-8",
    )?);
    let manifest = PuppetArtifactManifest {
        schema: "poche.puppet.artifact-manifest.v3".to_owned(),
        executable_revision: env!("POCHE_PUPPET_GIT_REVISION").to_owned(),
        executable_worktree: env!("POCHE_PUPPET_GIT_WORKTREE").to_owned(),
        run_id: report.run_id.clone(),
        scenario: report.scenario.clone(),
        surface: report.surface.clone(),
        status: report.status.clone(),
        final_revision: report.final_revision,
        qualification: qualification(&report),
        evidence_boundary: report.evidence_boundary.clone(),
        public_history_hash: report.public_history_hash.clone(),
        contact_sheet: "index.html".to_owned(),
        captures: capture_index,
        files,
    };
    write_json(&temporary_path.join("manifest.json"), &manifest)?;
    fs::rename(&temporary_path, &final_path).map_err(evidence_error)?;
    temporary.published = true;
    rebuild_catalog(&options.artifact_root)?;
    Ok(report)
}

fn qualification(report: &PuppetRunReport) -> String {
    match report.surface.as_str() {
        "headless" => "Certified semantic evidence only; no graphical claim.".to_owned(),
        "native" => "Certified semantic evidence plus signed same-player windowless Bevy captures, private transfer, and requester persistence.".to_owned(),
        "web" => "Certified semantic evidence plus a qualified real-browser UI run whose four-part bundles use signed same-player private transfer and requester persistence.".to_owned(),
        _ => "Unknown puppet surface qualification; inspect the evidence boundary.".to_owned(),
    }
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

fn index_capture(
    run_root: &Path,
    persisted: &PersistedCapture,
    evidence: &PuppetCaptureEvidence,
) -> Result<PuppetArtifactCapture, PuppetError> {
    let capture_root = persisted
        .directory
        .strip_prefix(run_root)
        .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
    let manifest_path = persisted
        .manifest_path
        .strip_prefix(run_root)
        .map_err(|_| capture_error(poche_capture::CapturePipelineError::Storage))?;
    let mut preview_path = None;
    let mut structural_paths = Vec::new();
    for entry in &persisted.manifest.entries {
        let path = portable_relative(&capture_root.join(&entry.relative_path));
        if entry.representation == poche_protocol::CaptureRepresentationWire::Png {
            preview_path = Some(path);
        } else {
            structural_paths.push(path);
        }
    }
    Ok(PuppetArtifactCapture {
        label: evidence.label.clone(),
        request_id: evidence.request_id.clone(),
        requester_device_id: evidence.requester_device_id.clone(),
        provider_device_id: evidence.provider_device_id.clone(),
        requested_revision: evidence.requested_revision,
        captured_revision: evidence.captured_revision,
        projection_hash: evidence.projection_hash.clone(),
        scene_hash: evidence.scene_hash.clone(),
        provider_kind: evidence.provider_kind.clone(),
        representations: evidence.representation.clone(),
        qualification: format!("{:?}", persisted.manifest.qualification),
        viewport_width: persisted.manifest.surface.viewport.width_pixels,
        viewport_height: persisted.manifest.surface.viewport.height_pixels,
        framebuffer_width: persisted.manifest.surface.framebuffer_width,
        framebuffer_height: persisted.manifest.surface.framebuffer_height,
        scale_milli: persisted.manifest.surface.scale_milli,
        camera_present: persisted.manifest.surface.camera.is_some(),
        windowless: evidence.windowless,
        transferred_bytes: evidence.transferred_bytes,
        transfer_chunks: evidence.transfer_chunks,
        manifest_path: portable_relative(manifest_path),
        preview_path,
        structural_paths,
    })
}

fn render_run_contact_sheet(
    report: &PuppetRunReport,
    captures: &[PuppetArtifactCapture],
) -> Vec<u8> {
    let cards = captures.iter().map(render_capture_card).collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Poche puppet evidence — {}</title>{}</head><body><header><p class=\"eyebrow\">POCHE PUPPET EVIDENCE</p><h1>{}</h1><p>{} · {} · seed {} · revision {} · {} public events</p><p>{}</p><nav><a href=\"run.json\">run.json</a><a href=\"steps.ndjson\">steps.ndjson</a><a href=\"manifest.json\">manifest.json</a></nav></header><main>{}</main><footer>Formal Alloy/NuSMV/Prolog evidence is release/developer evidence linked by repository revision; it was not executed by this page request.</footer></body></html>",
        escape_html(&report.run_id),
        contact_sheet_style(),
        escape_html(&report.scenario),
        escape_html(&report.surface),
        escape_html(&report.transport),
        report.seed,
        report.final_revision,
        report.public_history_events,
        escape_html(&report.evidence_boundary),
        if cards.is_empty() {
            "<section class=\"empty\"><h2>Headless semantic run</h2><p>This run contains no graphical figures. Use run.json and steps.ndjson for exact device observations and committed actions.</p></section>".to_owned()
        } else {
            cards
        }
    )
    .into_bytes()
}

fn render_capture_card(capture: &PuppetArtifactCapture) -> String {
    use std::fmt::Write as _;

    let preview = capture.preview_path.as_ref().map_or_else(
        || "<div class=\"no-preview\">No raster preview</div>".to_owned(),
        |path| {
            format!(
                "<a class=\"preview\" href=\"{}\"><img src=\"{}\" alt=\"{}\"></a>",
                escape_html(path),
                escape_html(path),
                escape_html(&capture.label)
            )
        },
    );
    let structures = capture
        .structural_paths
        .iter()
        .fold(String::new(), |mut output, path| {
            write!(
                output,
                "<a href=\"{}\">{}</a>",
                escape_html(path),
                escape_html(
                    Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("structure")
                )
            )
            .expect("writing to String cannot fail");
            output
        });
    format!(
        "<article>{}<div class=\"copy\"><p class=\"eyebrow\">REVISION {} · {}</p><h2>{}</h2><dl><dt>request</dt><dd>{}</dd><dt>devices</dt><dd>{} → {}</dd><dt>projection</dt><dd>{}</dd><dt>scene</dt><dd>{}</dd><dt>surface</dt><dd>{}×{} @ {}‰ · windowless {}</dd><dt>transfer</dt><dd>{} bytes · {} chunks</dd></dl><nav><a href=\"{}\">capture manifest</a>{}</nav></div></article>",
        preview,
        capture.captured_revision,
        escape_html(&capture.provider_kind),
        escape_html(&capture.label),
        escape_html(&capture.request_id),
        short_id(&capture.requester_device_id),
        short_id(&capture.provider_device_id),
        short_id(&capture.projection_hash),
        capture
            .scene_hash
            .as_deref()
            .map_or("none".to_owned(), short_id),
        capture.viewport_width,
        capture.viewport_height,
        capture.scale_milli,
        capture.windowless,
        capture.transferred_bytes,
        capture.transfer_chunks,
        escape_html(&capture.manifest_path),
        structures
    )
}

fn contact_sheet_style() -> &'static str {
    "<style>:root{color-scheme:dark;font:16px system-ui,sans-serif;background:#08120c;color:#eef6f0}*{box-sizing:border-box}body{max-width:1500px;margin:auto;padding:1rem}header,article,.empty{border:1px solid #31543b;border-radius:.6rem;background:#0d1d13}header{padding:1rem 1.25rem;margin-bottom:1rem}h1,h2,p{margin:.25rem 0 .65rem}.eyebrow{color:#99c9a8;font-size:.72rem;font-weight:800;letter-spacing:.12em}nav{display:flex;gap:.7rem;flex-wrap:wrap}a{color:#8fe1aa}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(34rem,1fr));gap:1rem}article{overflow:hidden}.preview{display:block;background:#020503}.preview img{display:block;width:100%;height:auto}.copy{padding:.8rem 1rem}dl{display:grid;grid-template-columns:max-content 1fr;gap:.25rem .7rem;font:12px ui-monospace,monospace}dt{color:#99c9a8}dd{margin:0;overflow-wrap:anywhere}.copy nav a{margin-right:.7rem}.empty{padding:1rem}footer{margin-top:1rem;color:#a5b4aa;font-size:.8rem}@media(max-width:650px){main{grid-template-columns:1fr}dl{grid-template-columns:1fr}.copy{padding:.65rem}}</style>"
}

fn portable_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn rebuild_catalog(root: &Path) -> Result<(), PuppetError> {
    let mut runs = Vec::new();
    for entry in fs::read_dir(root).map_err(evidence_error)? {
        let entry = entry.map_err(evidence_error)?;
        let directory = entry.path();
        if !directory.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let manifest_path = directory.join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest_bytes = fs::read(&manifest_path).map_err(evidence_error)?;
        let schema = serde_json::from_slice::<serde_json::Value>(&manifest_bytes)
            .ok()
            .and_then(|value| value["schema"].as_str().map(str::to_owned));
        if schema.as_deref() != Some("poche.puppet.artifact-manifest.v3") {
            continue;
        }
        let manifest: PuppetArtifactManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|_| invalid_catalog())?;
        verify_manifest_files(&directory, &manifest)?;
        let directory_name = entry.file_name().to_string_lossy().into_owned();
        runs.push(PuppetArtifactCatalogRun {
            run_id: manifest.run_id,
            directory: directory_name.clone(),
            scenario: manifest.scenario,
            surface: manifest.surface,
            status: manifest.status,
            final_revision: manifest.final_revision,
            executable_revision: manifest.executable_revision,
            public_history_hash: manifest.public_history_hash,
            contact_sheet: format!("{directory_name}/{}", manifest.contact_sheet),
            captures: manifest
                .captures
                .into_iter()
                .map(|mut capture| {
                    capture.manifest_path = format!("{directory_name}/{}", capture.manifest_path);
                    capture.preview_path = capture
                        .preview_path
                        .map(|path| format!("{directory_name}/{path}"));
                    capture.structural_paths = capture
                        .structural_paths
                        .into_iter()
                        .map(|path| format!("{directory_name}/{path}"))
                        .collect();
                    capture
                })
                .collect(),
            evidence_boundary: manifest.evidence_boundary,
        });
    }
    runs.sort_by(|left, right| {
        left.run_id
            .cmp(&right.run_id)
            .then(left.directory.cmp(&right.directory))
    });
    let contact_sheet = render_catalog_contact_sheet(&runs);
    let sheet_hash = blake3::hash(&contact_sheet).to_hex().to_string();
    write_replace(&root.join("index.html"), &contact_sheet)?;
    let catalog = PuppetArtifactCatalog {
        schema: "poche.puppet.artifact-catalog.v1".to_owned(),
        generated_from_verified_manifests: true,
        contact_sheet: "index.html".to_owned(),
        contact_sheet_blake3: sheet_hash,
        runs,
    };
    write_json_replace(&root.join("catalog.json"), &catalog)
}

fn verify_manifest_files(
    directory: &Path,
    manifest: &PuppetArtifactManifest,
) -> Result<(), PuppetError> {
    let mut unique = BTreeSet::new();
    for file in &manifest.files {
        if !safe_relative(&file.path) || !unique.insert(file.path.as_str()) {
            return Err(invalid_catalog());
        }
        let bytes = fs::read(directory.join(&file.path)).map_err(evidence_error)?;
        if u64::try_from(bytes.len()).map_err(|_| invalid_catalog())? != file.bytes
            || blake3::hash(&bytes).to_hex().as_str() != file.blake3
        {
            return Err(invalid_catalog());
        }
    }
    if !unique.contains(manifest.contact_sheet.as_str()) {
        return Err(invalid_catalog());
    }
    for capture in &manifest.captures {
        if !unique.contains(capture.manifest_path.as_str())
            || capture
                .preview_path
                .as_deref()
                .is_some_and(|path| !unique.contains(path))
            || capture
                .structural_paths
                .iter()
                .any(|path| !unique.contains(path.as_str()))
        {
            return Err(invalid_catalog());
        }
    }
    Ok(())
}

fn safe_relative(value: &str) -> bool {
    let path = Path::new(value);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn render_catalog_contact_sheet(runs: &[PuppetArtifactCatalogRun]) -> Vec<u8> {
    use std::fmt::Write as _;

    let summaries = runs
        .iter()
        .fold(String::new(), |mut output, run| {
            write!(
                output,
                "<li><a href=\"{}\"><strong>{}</strong></a><span> — {} · {} · {} captures · revision {}</span></li>",
                escape_html(&run.contact_sheet),
                escape_html(&run.run_id),
                escape_html(&run.surface),
                escape_html(&run.executable_revision),
                run.captures.len(),
                run.final_revision
            )
            .expect("writing to String cannot fail");
            output
        })
        ;
    let cards = runs
        .iter()
        .flat_map(|run| run.captures.iter())
        .map(render_capture_card)
        .collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Poche puppet artifact catalog</title>{}</head><body><header><p class=\"eyebrow\">VERIFIED CROSS-SURFACE CATALOG</p><h1>Poche puppet evidence</h1><p>Every linked run was re-hashed from its manifest before this catalog was generated.</p><nav><a href=\"catalog.json\">catalog.json</a></nav><ul>{}</ul></header><main>{}</main><footer>Each card states its renderer qualification and evidence boundary in the linked run. Visual evidence complements; it does not replace the certified semantic transcript or formal release evidence.</footer></body></html>",
        contact_sheet_style(),
        summaries,
        if cards.is_empty() {
            "<section class=\"empty\"><h2>No graphical captures yet</h2></section>".to_owned()
        } else {
            cards
        }
    )
    .into_bytes()
}

fn short_id(value: &str) -> String {
    let length = value.len().min(12);
    escape_html(&value[..length])
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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

fn write_json_replace(path: &Path, value: &impl Serialize) -> Result<(), PuppetError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid_catalog())?;
    let mut terminated = bytes;
    terminated.push(b'\n');
    write_replace(path, &terminated)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), PuppetError> {
    let mut file = create_new(path)?;
    file.write_all(bytes).map_err(evidence_error)?;
    file.flush().map_err(evidence_error)
}

fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), PuppetError> {
    let parent = path.parent().ok_or_else(invalid_catalog)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(invalid_catalog)?,
        std::process::id()
    ));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(evidence_error)?;
        file.write_all(bytes).map_err(evidence_error)?;
        file.flush().map_err(evidence_error)?;
    }
    if path.exists() {
        fs::remove_file(path).map_err(evidence_error)?;
    }
    fs::rename(temporary, path).map_err(evidence_error)
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

const fn invalid_catalog() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::EvidenceIo,
        "puppet artifact catalog verification failed",
    )
}

use std::{
    collections::BTreeSet,
    fmt, fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use image::{ColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage, codecs::png::PngEncoder};
use poche_protocol::{
    CaptureArtifactId, CaptureDenialReasonWire, CaptureProgressStageWire,
    CaptureProviderAdvertisementWire, CaptureProviderKindWire, CaptureRepresentationWire,
    CaptureRequestId, CaptureRequestWire, CaptureViewportWire, MAX_CAPTURE_ARTIFACT_BYTES,
    MAX_CAPTURE_ARTIFACTS, SemanticHash,
};
use serde::{Deserialize, Serialize};

const CAPTION_HEIGHT: u32 = 48;

/// Strength and provenance of one visual/structural observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureQualification {
    RuntimeGenerated,
    BrowserHarness,
    UserConfirmed,
}

/// Renderer-neutral integer camera metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureCameraMetadata {
    pub position_millimetres: [i64; 3],
    pub rotation_milliradians: [i32; 3],
    pub vertical_fov_millidegrees: u32,
}

/// Exact surface geometry accompanying raw provider data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSurfaceMetadata {
    pub provider_kind: CaptureProviderKindWire,
    pub viewport: CaptureViewportWire,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub scale_milli: u32,
    pub camera: Option<CaptureCameraMetadata>,
}

/// One renderer-produced artifact before validation and normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCaptureArtifact {
    pub artifact_id: CaptureArtifactId,
    pub representation: CaptureRepresentationWire,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub expected_source_hash: Option<SemanticHash>,
}

/// Complete renderer-neutral provider result. Providers do not choose paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCaptureBundle {
    pub figure_id: String,
    pub caption: String,
    pub captured_revision: u64,
    pub projection_hash: SemanticHash,
    pub scene_hash: Option<SemanticHash>,
    pub surface: CaptureSurfaceMetadata,
    pub qualification: CaptureQualification,
    pub cancelled: bool,
    pub artifacts: Vec<RawCaptureArtifact>,
}

/// Asynchronous provider result polled without blocking a renderer loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureProviderPoll {
    Pending(CaptureProgressStageWire),
    Ready(Box<RawCaptureBundle>),
    Denied(CaptureDenialReasonWire),
}

/// Renderer-neutral provider port. The caller verifies signatures and session
/// authorization before `begin_capture`; implementations only capture their
/// own exact device surface and never persist final evidence paths.
pub trait CaptureProvider {
    fn advertisement(&self) -> &CaptureProviderAdvertisementWire;

    fn begin_capture(&mut self, request: CaptureRequestWire) -> Result<(), CapturePipelineError>;

    fn poll_capture(
        &mut self,
        request_id: &CaptureRequestId,
    ) -> Result<CaptureProviderPoll, CapturePipelineError>;

    fn cancel_capture(&mut self, request_id: &CaptureRequestId)
    -> Result<(), CapturePipelineError>;
}

/// One normalized persisted artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureManifestEntry {
    pub artifact_id: CaptureArtifactId,
    pub representation: CaptureRepresentationWire,
    pub media_type: String,
    pub relative_path: String,
    pub byte_length: u64,
    pub content_hash: SemanticHash,
    pub source_width: Option<u32>,
    pub source_height: Option<u32>,
    pub output_width: Option<u32>,
    pub output_height: Option<u32>,
}

/// Deterministic evidence manifest emitted beside artifacts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureManifest {
    pub schema_version: u16,
    pub figure_id: String,
    pub caption: String,
    pub captured_revision: u64,
    pub projection_hash: SemanticHash,
    pub scene_hash: Option<SemanticHash>,
    pub surface: CaptureSurfaceMetadata,
    pub qualification: CaptureQualification,
    pub entries: Vec<CaptureManifestEntry>,
}

/// Successful publication of one atomically persisted figure directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistedCapture {
    pub directory: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: CaptureManifest,
}

/// One secret/private marker supplied by the caller without exposing it in
/// errors or manifests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateMarker(Vec<u8>);

impl PrivateMarker {
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }
}

/// Central pipeline configuration.
#[derive(Clone, Debug)]
pub struct CapturePipeline {
    root: PathBuf,
    private_markers: Vec<PrivateMarker>,
}

impl CapturePipeline {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            private_markers: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_private_markers(mut self, markers: Vec<PrivateMarker>) -> Self {
        self.private_markers = markers;
        self
    }

    pub fn persist(
        &self,
        bundle: &RawCaptureBundle,
    ) -> Result<PersistedCapture, CapturePipelineError> {
        validate_bundle(bundle)?;
        if bundle.cancelled {
            return Err(CapturePipelineError::Cancelled);
        }
        self.scan(bundle.figure_id.as_bytes())?;
        self.scan(bundle.caption.as_bytes())?;
        for artifact in &bundle.artifacts {
            self.scan(&artifact.bytes)?;
        }

        fs::create_dir_all(&self.root).map_err(|_| CapturePipelineError::Storage)?;
        let destination = self.root.join(&bundle.figure_id);
        if destination.exists() {
            return Err(CapturePipelineError::DuplicateId);
        }
        let temporary = tempfile::Builder::new()
            .prefix(".poche-capture-")
            .tempdir_in(&self.root)
            .map_err(|_| CapturePipelineError::Storage)?;
        let mut entries = Vec::with_capacity(bundle.artifacts.len());
        for artifact in &bundle.artifacts {
            let normalized = normalize_artifact(artifact, bundle)?;
            self.scan(&normalized.bytes)?;
            let file_name = format!(
                "{}.{}",
                artifact.artifact_id.as_str(),
                representation_extension(artifact.representation)
            );
            fs::write(temporary.path().join(&file_name), &normalized.bytes)
                .map_err(|_| CapturePipelineError::Storage)?;
            entries.push(CaptureManifestEntry {
                artifact_id: artifact.artifact_id.clone(),
                representation: artifact.representation,
                media_type: artifact.media_type.clone(),
                relative_path: file_name,
                byte_length: u64::try_from(normalized.bytes.len())
                    .map_err(|_| CapturePipelineError::Oversize)?,
                content_hash: SemanticHash(*blake3::hash(&normalized.bytes).as_bytes()),
                source_width: normalized.source_dimensions.map(|value| value.0),
                source_height: normalized.source_dimensions.map(|value| value.1),
                output_width: normalized.output_dimensions.map(|value| value.0),
                output_height: normalized.output_dimensions.map(|value| value.1),
            });
        }
        let manifest = CaptureManifest {
            schema_version: 1,
            figure_id: bundle.figure_id.clone(),
            caption: bundle.caption.clone(),
            captured_revision: bundle.captured_revision,
            projection_hash: bundle.projection_hash,
            scene_hash: bundle.scene_hash,
            surface: bundle.surface.clone(),
            qualification: bundle.qualification,
            entries,
        };
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).map_err(|_| CapturePipelineError::Encoding)?;
        self.scan(&manifest_bytes)?;
        fs::write(temporary.path().join("manifest.json"), manifest_bytes)
            .map_err(|_| CapturePipelineError::Storage)?;

        let temporary_path = temporary.keep();
        if fs::rename(&temporary_path, &destination).is_err() {
            let _ = fs::remove_dir_all(&temporary_path);
            return Err(CapturePipelineError::Storage);
        }
        Ok(PersistedCapture {
            manifest_path: destination.join("manifest.json"),
            directory: destination,
            manifest,
        })
    }

    fn scan(&self, bytes: &[u8]) -> Result<(), CapturePipelineError> {
        if self.private_markers.iter().any(|marker| {
            !marker.0.is_empty()
                && bytes
                    .windows(marker.0.len())
                    .any(|window| window == marker.0)
        }) {
            Err(CapturePipelineError::PrivateData)
        } else {
            Ok(())
        }
    }
}

/// Stable failures that never include rejected bytes, paths, or private text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePipelineError {
    InvalidMetadata,
    InvalidArtifact,
    InvalidImage,
    UnsafeId,
    DuplicateId,
    HashMismatch,
    PrivateData,
    Oversize,
    Cancelled,
    Encoding,
    Storage,
}

impl fmt::Display for CapturePipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMetadata => "capture metadata is invalid",
            Self::InvalidArtifact => "capture artifact is invalid",
            Self::InvalidImage => "capture image is invalid",
            Self::UnsafeId => "capture identifier is unsafe",
            Self::DuplicateId => "capture identifier already exists",
            Self::HashMismatch => "capture hash does not match",
            Self::PrivateData => "capture contains private data",
            Self::Oversize => "capture exceeds the artifact limit",
            Self::Cancelled => "capture was cancelled",
            Self::Encoding => "capture encoding failed",
            Self::Storage => "capture storage failed",
        })
    }
}

impl std::error::Error for CapturePipelineError {}

struct NormalizedArtifact {
    bytes: Vec<u8>,
    source_dimensions: Option<(u32, u32)>,
    output_dimensions: Option<(u32, u32)>,
}

fn validate_bundle(bundle: &RawCaptureBundle) -> Result<(), CapturePipelineError> {
    if !safe_id(&bundle.figure_id) {
        return Err(CapturePipelineError::UnsafeId);
    }
    if bundle.caption.is_empty()
        || bundle.caption.len() > 160
        || bundle.caption.chars().any(char::is_control)
        || bundle.artifacts.is_empty()
        || bundle.artifacts.len() > MAX_CAPTURE_ARTIFACTS
        || bundle.surface.framebuffer_width == 0
        || bundle.surface.framebuffer_height == 0
        || bundle.surface.scale_milli == 0
        || bundle.surface.scale_milli > 16_000
        || bundle.surface.viewport.width_pixels == 0
        || bundle.surface.viewport.height_pixels == 0
        || bundle.surface.viewport.width_pixels > 16_384
        || bundle.surface.viewport.height_pixels > 16_384
    {
        return Err(CapturePipelineError::InvalidMetadata);
    }
    if let Some(camera) = &bundle.surface.camera
        && (camera.vertical_fov_millidegrees == 0 || camera.vertical_fov_millidegrees >= 180_000)
    {
        return Err(CapturePipelineError::InvalidMetadata);
    }
    let ids = bundle
        .artifacts
        .iter()
        .map(|artifact| artifact.artifact_id.clone())
        .collect::<BTreeSet<_>>();
    let representations = bundle
        .artifacts
        .iter()
        .map(|artifact| artifact.representation)
        .collect::<BTreeSet<_>>();
    if ids.len() != bundle.artifacts.len() || representations.len() != bundle.artifacts.len() {
        return Err(CapturePipelineError::DuplicateId);
    }
    let total_bytes = bundle.artifacts.iter().try_fold(0_u64, |total, artifact| {
        total.checked_add(u64::try_from(artifact.bytes.len()).ok()?)
    });
    if total_bytes.is_none_or(|total| total > MAX_CAPTURE_ARTIFACT_BYTES) {
        return Err(CapturePipelineError::Oversize);
    }
    for artifact in &bundle.artifacts {
        if !artifact.artifact_id.validate()
            || artifact.bytes.is_empty()
            || u64::try_from(artifact.bytes.len())
                .map_or(true, |length| length > MAX_CAPTURE_ARTIFACT_BYTES)
            || artifact.media_type != representation_media_type(artifact.representation)
        {
            return Err(CapturePipelineError::InvalidArtifact);
        }
        if artifact.expected_source_hash.is_some_and(|expected| {
            expected != SemanticHash(*blake3::hash(&artifact.bytes).as_bytes())
        }) {
            return Err(CapturePipelineError::HashMismatch);
        }
    }
    Ok(())
}

fn normalize_artifact(
    artifact: &RawCaptureArtifact,
    bundle: &RawCaptureBundle,
) -> Result<NormalizedArtifact, CapturePipelineError> {
    match artifact.representation {
        CaptureRepresentationWire::Png => normalize_png(&artifact.bytes, bundle),
        CaptureRepresentationWire::SemanticHtml => {
            std::str::from_utf8(&artifact.bytes)
                .map_err(|_| CapturePipelineError::InvalidArtifact)?;
            Ok(raw_artifact(&artifact.bytes))
        }
        CaptureRepresentationWire::AccessibilityTreeJson
        | CaptureRepresentationWire::LayoutJson => {
            let value: serde_json::Value = serde_json::from_slice(&artifact.bytes)
                .map_err(|_| CapturePipelineError::InvalidArtifact)?;
            let bytes =
                serde_json::to_vec_pretty(&value).map_err(|_| CapturePipelineError::Encoding)?;
            Ok(raw_artifact(&bytes))
        }
    }
}

fn raw_artifact(bytes: &[u8]) -> NormalizedArtifact {
    NormalizedArtifact {
        bytes: bytes.to_vec(),
        source_dimensions: None,
        output_dimensions: None,
    }
}

fn normalize_png(
    bytes: &[u8],
    bundle: &RawCaptureBundle,
) -> Result<NormalizedArtifact, CapturePipelineError> {
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|_| CapturePipelineError::InvalidImage)?
        .to_rgba8();
    let source = image.dimensions();
    if source
        != (
            bundle.surface.framebuffer_width,
            bundle.surface.framebuffer_height,
        )
        || source
            != (
                bundle.surface.viewport.width_pixels,
                bundle.surface.viewport.height_pixels,
            )
    {
        return Err(CapturePipelineError::InvalidImage);
    }
    let output_height = source
        .1
        .checked_add(CAPTION_HEIGHT)
        .ok_or(CapturePipelineError::Oversize)?;
    let mut captioned = RgbaImage::from_pixel(source.0, output_height, Rgba([246, 246, 242, 255]));
    image::imageops::overlay(&mut captioned, &image, 0, 0);
    draw_caption(&mut captioned, source.1, &bundle.caption);
    let mut encoded = Vec::new();
    PngEncoder::new(Cursor::new(&mut encoded))
        .write_image(
            captioned.as_raw(),
            source.0,
            output_height,
            ColorType::Rgba8.into(),
        )
        .map_err(|_| CapturePipelineError::Encoding)?;
    if u64::try_from(encoded.len()).map_or(true, |length| length > MAX_CAPTURE_ARTIFACT_BYTES) {
        return Err(CapturePipelineError::Oversize);
    }
    Ok(NormalizedArtifact {
        bytes: encoded,
        source_dimensions: Some(source),
        output_dimensions: Some((source.0, output_height)),
    })
}

fn draw_caption(image: &mut RgbaImage, source_height: u32, caption: &str) {
    let text = caption.to_ascii_uppercase();
    let max_chars = usize::try_from(image.width().saturating_sub(16) / 6).unwrap_or(0);
    for (index, character) in text.chars().take(max_chars).enumerate() {
        let Some(glyph) = glyph(character) else {
            continue;
        };
        let origin_x = 8 + u32::try_from(index).unwrap_or(0).saturating_mul(6);
        let origin_y = source_height.saturating_add(17);
        for (row, bits) in glyph.into_iter().enumerate() {
            for column in 0..5_u32 {
                if bits & (1 << (4 - column)) != 0 {
                    let y = origin_y.saturating_add(u32::try_from(row).unwrap_or(0));
                    if origin_x + column < image.width() && y < image.height() {
                        image.put_pixel(origin_x + column, y, Rgba([24, 32, 38, 255]));
                    }
                }
            }
        }
    }
}

fn glyph(character: char) -> Option<[u8; 7]> {
    Some(match character {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        ' ' => [0; 7],
        _ => return None,
    })
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

const fn representation_media_type(representation: CaptureRepresentationWire) -> &'static str {
    match representation {
        CaptureRepresentationWire::Png => "image/png",
        CaptureRepresentationWire::SemanticHtml => "text/html; charset=utf-8",
        CaptureRepresentationWire::AccessibilityTreeJson
        | CaptureRepresentationWire::LayoutJson => "application/json",
    }
}

const fn representation_extension(representation: CaptureRepresentationWire) -> &'static str {
    match representation {
        CaptureRepresentationWire::Png => "png",
        CaptureRepresentationWire::SemanticHtml => "html",
        CaptureRepresentationWire::AccessibilityTreeJson
        | CaptureRepresentationWire::LayoutJson => "json",
    }
}

/// Read and validate a manifest emitted by the pipeline.
pub fn read_manifest(path: &Path) -> Result<CaptureManifest, CapturePipelineError> {
    let bytes = fs::read(path).map_err(|_| CapturePipelineError::Storage)?;
    serde_json::from_slice(&bytes).map_err(|_| CapturePipelineError::Encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, Rgba([16, 96, 64, 255]));
        let mut bytes = Vec::new();
        PngEncoder::new(Cursor::new(&mut bytes))
            .write_image(image.as_raw(), width, height, ColorType::Rgba8.into())
            .unwrap();
        bytes
    }

    fn bundle(figure_id: &str, provider_kind: CaptureProviderKindWire) -> RawCaptureBundle {
        let image = png(64, 32);
        RawCaptureBundle {
            figure_id: figure_id.to_owned(),
            caption: "Round 3 bidding - Alice view".to_owned(),
            captured_revision: 42,
            projection_hash: SemanticHash([1; 32]),
            scene_hash: Some(SemanticHash([2; 32])),
            surface: CaptureSurfaceMetadata {
                provider_kind,
                viewport: CaptureViewportWire {
                    width_pixels: 64,
                    height_pixels: 32,
                },
                framebuffer_width: 64,
                framebuffer_height: 32,
                scale_milli: 1_000,
                camera: None,
            },
            qualification: CaptureQualification::RuntimeGenerated,
            cancelled: false,
            artifacts: vec![
                RawCaptureArtifact {
                    artifact_id: CaptureArtifactId::new(format!("{figure_id}-png")).unwrap(),
                    representation: CaptureRepresentationWire::Png,
                    media_type: "image/png".to_owned(),
                    expected_source_hash: Some(SemanticHash(*blake3::hash(&image).as_bytes())),
                    bytes: image,
                },
                RawCaptureArtifact {
                    artifact_id: CaptureArtifactId::new(format!("{figure_id}-layout")).unwrap(),
                    representation: CaptureRepresentationWire::LayoutJson,
                    media_type: "application/json".to_owned(),
                    bytes: br#"{"z":2,"a":1}"#.to_vec(),
                    expected_source_hash: None,
                },
            ],
        }
    }

    #[test]
    fn bevy_and_browser_payloads_share_manifest_semantics() {
        let root = tempfile::tempdir().unwrap();
        let pipeline = CapturePipeline::new(root.path());
        let native = pipeline
            .persist(&bundle("native", CaptureProviderKindWire::NativeBevy))
            .unwrap();
        let browser = pipeline
            .persist(&bundle("browser", CaptureProviderKindWire::BrowserHarness))
            .unwrap();

        assert_eq!(
            native.manifest.schema_version,
            browser.manifest.schema_version
        );
        assert_eq!(
            native.manifest.entries.len(),
            browser.manifest.entries.len()
        );
        for (native, browser) in native
            .manifest
            .entries
            .iter()
            .zip(&browser.manifest.entries)
        {
            assert_eq!(native.representation, browser.representation);
            assert_eq!(native.media_type, browser.media_type);
            assert_eq!(native.byte_length, browser.byte_length);
            assert_eq!(native.content_hash, browser.content_hash);
            assert_eq!(native.source_width, browser.source_width);
            assert_eq!(native.output_height, browser.output_height);
        }
    }

    #[test]
    fn caption_band_is_outside_and_does_not_change_source_pixels() {
        let root = tempfile::tempdir().unwrap();
        let persisted = CapturePipeline::new(root.path())
            .persist(&bundle("caption", CaptureProviderKindWire::NativeBevy))
            .unwrap();
        let entry = &persisted.manifest.entries[0];
        assert_eq!(entry.source_height, Some(32));
        assert_eq!(entry.output_height, Some(32 + CAPTION_HEIGHT));
        let output = image::open(persisted.directory.join(&entry.relative_path))
            .unwrap()
            .to_rgba8();
        assert_eq!(output.get_pixel(0, 0), &Rgba([16, 96, 64, 255]));
        assert_eq!(output.get_pixel(0, 31), &Rgba([16, 96, 64, 255]));
        assert_eq!(output.get_pixel(0, 32), &Rgba([246, 246, 242, 255]));
    }

    #[test]
    fn malformed_unsafe_duplicate_hash_private_cancel_and_republish_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let pipeline = CapturePipeline::new(root.path())
            .with_private_markers(vec![PrivateMarker::new(b"ROOM-SECRET".to_vec())]);

        let mut invalid = bundle("invalid", CaptureProviderKindWire::NativeBevy);
        invalid.artifacts[0].bytes = b"not png".to_vec();
        invalid.artifacts[0].expected_source_hash = None;
        assert_eq!(
            pipeline.persist(&invalid),
            Err(CapturePipelineError::InvalidImage)
        );
        assert!(!root.path().join("invalid").exists());

        let mut unsafe_bundle = bundle("safe", CaptureProviderKindWire::NativeBevy);
        unsafe_bundle.figure_id = "../escape".to_owned();
        assert_eq!(
            pipeline.persist(&unsafe_bundle),
            Err(CapturePipelineError::UnsafeId)
        );

        let mut duplicate = bundle("duplicate", CaptureProviderKindWire::NativeBevy);
        duplicate.artifacts[1].artifact_id = duplicate.artifacts[0].artifact_id.clone();
        assert_eq!(
            pipeline.persist(&duplicate),
            Err(CapturePipelineError::DuplicateId)
        );

        let mut mismatch = bundle("mismatch", CaptureProviderKindWire::NativeBevy);
        mismatch.artifacts[0].expected_source_hash = Some(SemanticHash([99; 32]));
        assert_eq!(
            pipeline.persist(&mismatch),
            Err(CapturePipelineError::HashMismatch)
        );

        let mut private = bundle("private", CaptureProviderKindWire::NativeBevy);
        private.artifacts[1].bytes = br#"{"value":"ROOM-SECRET"}"#.to_vec();
        assert_eq!(
            pipeline.persist(&private),
            Err(CapturePipelineError::PrivateData)
        );

        let mut cancelled = bundle("cancelled", CaptureProviderKindWire::NativeBevy);
        cancelled.cancelled = true;
        assert_eq!(
            pipeline.persist(&cancelled),
            Err(CapturePipelineError::Cancelled)
        );
        assert!(!root.path().join("cancelled").exists());

        let valid = bundle("once", CaptureProviderKindWire::NativeBevy);
        pipeline.persist(&valid).unwrap();
        assert_eq!(
            pipeline.persist(&valid),
            Err(CapturePipelineError::DuplicateId)
        );
        assert!(read_manifest(&root.path().join("once/manifest.json")).is_ok());
        assert!(
            fs::read_dir(root.path())
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".poche-capture-"))
        );
    }
}

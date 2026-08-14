// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Native graphical evidence composed through the certified-device bridge.
//!
//! The provider owns render bytes until the requester consumes the bounded
//! encrypted transfer. Only the requester-facing artifact stage receives a
//! reconstructed raw bundle; the Bevy provider never chooses output paths.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use ed25519_dalek::{Signer, SigningKey};
use poche_capture::{
    CaptureChunkDisposition, CaptureProvider, CaptureProviderPoll, CaptureTransferKey,
    CaptureTransferReceiver, CaptureTransferSender, RawCaptureArtifact, RawCaptureBundle,
    capture_transfer_descriptor,
};
use poche_native_ui::{
    NativeCaptureContext, NativeCaptureProvider, NativeLiveDevice, NativeRenderMode,
    NativeUiLaunchOptions, run_live,
};
use poche_player_client::{
    DeviceClientError, DeviceCooperationRequest, DeviceCooperationResult, DeviceProfile,
    LoopbackDeviceTransport, PlayerDeviceClient,
};
use poche_protocol::{
    CaptureArtifactDescriptorWire, CaptureConsentPolicyWire, CapturePrivacyWire,
    CaptureProviderAdvertisementWire, CaptureProviderKindWire, CaptureRepresentationWire,
    CaptureRequestId, CaptureResponseOutcomeWire, CaptureTransferId,
    DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
    DeviceSignatureIntentWire, PublicGamePhase, RoomId, RoomPhase, SignatureAlgorithm,
    SignatureBytes, UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
    UnsignedCaptureResponseWire, canonical_capture_provider_advertisement_bytes,
    canonical_capture_request_bytes, canonical_capture_response_bytes, capture_request_hash,
};
use poche_runtime::{RuntimeDeviceCooperationContext, RuntimeDeviceCooperationHandler};

use crate::{
    PuppetError, PuppetErrorCode, PuppetRunOptions,
    scenario::{Adapter, Client, PuppetCaptureEvidence},
};

const COOPERATION_NOW_UNIX_MS: u64 = 100;
const COOPERATION_EXPIRES_UNIX_MS: u64 = 10_000;
const CAPTURE_MAX_BYTES: u64 = 8 * 1024 * 1024;
const CAPTURE_CHUNK_BYTES: u32 = 24 * 1024;

pub(crate) struct PendingPuppetCapture {
    pub bundle: RawCaptureBundle,
    pub evidence: PuppetCaptureEvidence,
}

struct PreparedArtifact {
    descriptor: CaptureArtifactDescriptorWire,
    artifact: RawCaptureArtifact,
    sender: CaptureTransferSender,
    receiver_key: CaptureTransferKey,
}

pub(crate) struct PreparedCapture {
    bundle: RawCaptureBundle,
    artifacts: Vec<PreparedArtifact>,
}

struct NativeCaptureHandler {
    advertisement: CaptureProviderAdvertisementWire,
    adapter: Adapter,
    provider_profile: DeviceProfile,
    provider_key: SigningKey,
    mailbox: Arc<Mutex<Option<PreparedCapture>>>,
    show_window: bool,
}

impl RuntimeDeviceCooperationHandler for NativeCaptureHandler {
    fn cooperate(
        &mut self,
        _context: &RuntimeDeviceCooperationContext,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        let DeviceCooperationRequest::Capture(request) = request else {
            return Err(DeviceClientError::ProtocolViolation);
        };
        let mut provider = NativeCaptureProvider::new(self.advertisement.clone());
        provider
            .begin_capture(request.clone())
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let mut result = provider.clone();
        let provider_client = PlayerDeviceClient::new(
            self.provider_profile.clone(),
            LoopbackDeviceTransport::new(self.adapter.clone()),
        )?;
        let live = NativeLiveDevice::connect(provider_client, request.room_id.clone())
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if live.observation().projection.current_revision != request.observed_revision {
            return Err(DeviceClientError::StaleRevision);
        }
        let projection_hash = live.observation().projection_hash;
        run_live(
            NativeUiLaunchOptions {
                // Leave enough frames after the 900-ms scene warm-up for GPU
                // readback to complete before the bounded runner exits.
                exit_after_seconds: Some(3.0),
                render_mode: if self.show_window {
                    NativeRenderMode::InteractiveWindow
                } else {
                    NativeRenderMode::WindowlessImage
                },
                capture_provider: Some(provider),
                capture_context: Some(NativeCaptureContext {
                    current_revision: request.observed_revision,
                    projection_hash,
                }),
                external_tracing: true,
                ..NativeUiLaunchOptions::default()
            },
            live,
        )
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
        let bundle = match result
            .poll_capture(&request.request_id)
            .map_err(|_| DeviceClientError::ProtocolViolation)?
        {
            CaptureProviderPoll::Ready(bundle) => *bundle,
            CaptureProviderPoll::Pending(_) | CaptureProviderPoll::Denied(_) => {
                return Err(DeviceClientError::ProtocolViolation);
            }
        };
        let (prepared, descriptors) = prepare_transfers(&request, bundle)?;
        let unsigned = UnsignedCaptureResponseWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash: capture_request_hash(&request.unsigned())
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
            room_id: request.room_id.clone(),
            membership_epoch: request.membership_epoch,
            player_id: request.player_id.clone(),
            requester_device_id: request.requester_device_id.clone(),
            provider_device_id: request.provider_device_id.clone(),
            outcome: CaptureResponseOutcomeWire::Accepted {
                artifacts: descriptors,
            },
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: request.provider_device_id,
            },
        };
        let signature = sign(
            &self.provider_key,
            &canonical_capture_response_bytes(&unsigned)
                .map_err(|_| DeviceClientError::ProtocolViolation)?,
        )?;
        let response = unsigned
            .attach_signature(signature)
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let mut mailbox = self
            .mailbox
            .lock()
            .map_err(|_| DeviceClientError::TransportUnavailable)?;
        if mailbox.replace(prepared).is_some() {
            return Err(DeviceClientError::ProtocolViolation);
        }
        Ok(DeviceCooperationResult::Capture(response))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the harness passes the two exact device identities and keys separately to preserve the authorization boundary"
)]
pub(crate) struct NativeCaptureSession {
    room_id: RoomId,
    requester_key: SigningKey,
    provider_profile: DeviceProfile,
    mailbox: Arc<Mutex<Option<PreparedCapture>>>,
    captured_labels: BTreeSet<&'static str>,
    show_window: bool,
    seed: u64,
}

impl NativeCaptureSession {
    pub(crate) fn register(
        options: &PuppetRunOptions,
        room_id: &RoomId,
        adapter: &Adapter,
        requester_key: &SigningKey,
        provider_profile: &DeviceProfile,
        provider_key: &SigningKey,
    ) -> Result<Self, PuppetError> {
        adapter
            .set_cooperation_now_unix_ms(COOPERATION_NOW_UNIX_MS)
            .map_err(device_error)?;
        let advertisement = signed_advertisement(room_id, provider_profile, provider_key)?;
        let mailbox = Arc::new(Mutex::new(None));
        adapter
            .register_capture_provider(
                provider_profile,
                advertisement.clone(),
                NativeCaptureHandler {
                    advertisement,
                    adapter: adapter.clone(),
                    provider_profile: provider_profile.clone(),
                    provider_key: provider_key.clone(),
                    mailbox: Arc::clone(&mailbox),
                    show_window: options.show_native_window,
                },
            )
            .map_err(device_error)?;
        Ok(Self {
            room_id: room_id.clone(),
            requester_key: requester_key.clone(),
            provider_profile: provider_profile.clone(),
            mailbox,
            captured_labels: BTreeSet::new(),
            show_window: options.show_native_window,
            seed: options.seed,
        })
    }

    pub(crate) fn capture_next_checkpoint(
        &mut self,
        requester: &mut Client,
    ) -> Result<Option<PendingPuppetCapture>, PuppetError> {
        let observation = requester.observe(&self.room_id).map_err(device_error)?;
        let Some((label, caption)) = checkpoint(&observation) else {
            return Ok(None);
        };
        if !self.captured_labels.insert(label) {
            return Ok(None);
        }
        self.capture(
            requester,
            label,
            caption,
            observation.projection.current_revision,
        )
        .map(Some)
    }

    fn capture(
        &mut self,
        requester: &mut Client,
        label: &'static str,
        caption: &'static str,
        revision: u64,
    ) -> Result<PendingPuppetCapture, PuppetError> {
        let unsigned = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new(format!(
                "native-{label}-seed-{}-revision-{revision}",
                self.seed
            ))
            .map_err(|_| invalid_capture())?,
            room_id: self.room_id.clone(),
            membership_epoch: 1,
            player_id: requester.profile().player_id.clone(),
            requester_device_id: requester.profile().device_id.clone(),
            provider_device_id: self.provider_profile.device_id.clone(),
            observed_revision: revision,
            expires_at_unix_ms: COOPERATION_EXPIRES_UNIX_MS,
            replay_nonce: format!("native-{label}-nonce-{}-{revision}", self.seed),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            viewport: None,
            label: caption.to_owned(),
            max_total_bytes: CAPTURE_MAX_BYTES,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: requester.profile().device_id.clone(),
            },
        };
        let request = unsigned
            .clone()
            .attach_signature(
                sign(
                    &self.requester_key,
                    &canonical_capture_request_bytes(&unsigned).map_err(|_| invalid_capture())?,
                )
                .map_err(|_| invalid_capture())?,
            )
            .map_err(|_| invalid_capture())?;
        let result = requester
            .cooperate(
                &self.provider_profile.device_id,
                DeviceCooperationRequest::Capture(request.clone()),
            )
            .map_err(device_error)?;
        let DeviceCooperationResult::Capture(response) = result;
        let response_descriptors = match response.outcome {
            CaptureResponseOutcomeWire::Accepted { artifacts } => artifacts,
            CaptureResponseOutcomeWire::Denied { .. } => {
                return Err(PuppetError::new(
                    PuppetErrorCode::ActionDenied,
                    "native capture provider denied the authorized request",
                ));
            }
        };
        let prepared = self
            .mailbox
            .lock()
            .map_err(|_| device_error(DeviceClientError::TransportUnavailable))?
            .take()
            .ok_or_else(invalid_capture)?;
        receive_prepared_capture(
            &request,
            label,
            &response_descriptors,
            prepared,
            !self.show_window,
            "native_bevy",
            "png",
        )
    }
}

fn checkpoint(
    observation: &poche_player_client::DeviceObservation,
) -> Option<(&'static str, &'static str)> {
    if observation.projection.payload.phase == RoomPhase::PostGame {
        return Some(("terminal", "Terminal native player view"));
    }
    let game = observation.projection.payload.public_game_state.as_ref()?;
    match game.phase {
        PublicGamePhase::Bidding => Some(("bidding", "Bidding native player view")),
        PublicGamePhase::Playing if !game.current_trick.is_empty() => {
            Some(("trick-in-progress", "Trick in progress native player view"))
        }
        PublicGamePhase::Playing if game.tricks_won.iter().any(|tricks| *tricks > 0) => {
            Some(("trick-resolved", "Resolved trick native player view"))
        }
        PublicGamePhase::Playing => Some(("card-selection", "Card selection native player view")),
        PublicGamePhase::Scoring => Some(("scoring", "Score sheet native player view")),
        PublicGamePhase::AwaitingDeal | PublicGamePhase::Finished => None,
    }
}

fn signed_advertisement(
    room_id: &RoomId,
    provider: &DeviceProfile,
    key: &SigningKey,
) -> Result<CaptureProviderAdvertisementWire, PuppetError> {
    let unsigned = UnsignedCaptureProviderAdvertisementWire {
        schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
        room_id: room_id.clone(),
        membership_epoch: 1,
        player_id: provider.player_id.clone(),
        provider_device_id: provider.device_id.clone(),
        provider_kind: CaptureProviderKindWire::NativeBevy,
        representations: vec![CaptureRepresentationWire::Png],
        privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
        consent_policy: CaptureConsentPolicyWire::HarnessOnly,
        max_total_bytes: CAPTURE_MAX_BYTES,
        advertisement_sequence: 1,
        expires_at_unix_ms: COOPERATION_EXPIRES_UNIX_MS,
        signature_intent: DeviceSignatureIntentWire {
            domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: provider.device_id.clone(),
        },
    };
    let signature = sign(
        key,
        &canonical_capture_provider_advertisement_bytes(&unsigned)
            .map_err(|_| invalid_capture())?,
    )
    .map_err(|_| invalid_capture())?;
    unsigned
        .attach_signature(signature)
        .map_err(|_| invalid_capture())
}

pub(crate) fn prepare_transfers(
    request: &poche_protocol::CaptureRequestWire,
    mut bundle: RawCaptureBundle,
) -> Result<(PreparedCapture, Vec<CaptureArtifactDescriptorWire>), DeviceClientError> {
    let request_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    let raw_artifacts = std::mem::take(&mut bundle.artifacts);
    let mut prepared = Vec::with_capacity(raw_artifacts.len());
    let mut descriptors = Vec::with_capacity(raw_artifacts.len());
    for (index, mut artifact) in raw_artifacts.into_iter().enumerate() {
        let transfer_id =
            CaptureTransferId::new(format!("{}-artifact-{index}", request.request_id.as_str()))
                .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let descriptor =
            capture_transfer_descriptor(transfer_id, &artifact.bytes, CAPTURE_CHUNK_BYTES)
                .map_err(|_| DeviceClientError::ProtocolViolation)?;
        // Content keys are fresh secret material. They are handed to the
        // requester through the harness's protected in-process channel and
        // never derived from public request/descriptor fields.
        let mut key_bytes = [0_u8; 32];
        getrandom::fill(&mut key_bytes).map_err(|_| DeviceClientError::KeyUnavailable)?;
        let sender = CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            request.expires_at_unix_ms,
            std::mem::take(&mut artifact.bytes),
            CaptureTransferKey::new(key_bytes),
            256,
        )
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
        let artifact_descriptor = CaptureArtifactDescriptorWire {
            artifact_id: artifact.artifact_id.clone(),
            representation: artifact.representation,
            provider_kind: bundle.surface.provider_kind,
            captured_revision: bundle.captured_revision,
            projection_hash: bundle.projection_hash,
            scene_hash: bundle.scene_hash,
            viewport: Some(bundle.surface.viewport),
            transfer: descriptor,
        };
        descriptors.push(artifact_descriptor.clone());
        prepared.push(PreparedArtifact {
            descriptor: artifact_descriptor,
            artifact,
            sender,
            receiver_key: CaptureTransferKey::new(key_bytes),
        });
    }
    Ok((
        PreparedCapture {
            bundle,
            artifacts: prepared,
        },
        descriptors,
    ))
}

pub(crate) fn receive_prepared_capture(
    request: &poche_protocol::CaptureRequestWire,
    label: &str,
    response_descriptors: &[CaptureArtifactDescriptorWire],
    mut prepared: PreparedCapture,
    windowless: bool,
    provider_kind: &str,
    representation: &str,
) -> Result<PendingPuppetCapture, PuppetError> {
    let expected = prepared
        .artifacts
        .iter()
        .map(|artifact| artifact.descriptor.clone())
        .collect::<Vec<_>>();
    if response_descriptors != expected {
        return Err(invalid_capture());
    }
    let request_hash = capture_request_hash(&request.unsigned()).map_err(|_| invalid_capture())?;
    let mut transferred_bytes = 0_u64;
    let mut transfer_chunks = 0_u32;
    for mut prepared_artifact in prepared.artifacts {
        let mut receiver = CaptureTransferReceiver::new(
            prepared_artifact.descriptor.transfer.clone(),
            request_hash,
            request.expires_at_unix_ms,
            prepared_artifact.receiver_key,
        )
        .map_err(|_| invalid_capture())?;
        while let Some(chunk) = prepared_artifact
            .sender
            .next_chunk(COOPERATION_NOW_UNIX_MS)
            .map_err(|_| invalid_capture())?
        {
            let disposition = receiver
                .accept(COOPERATION_NOW_UNIX_MS, &chunk)
                .map_err(|_| invalid_capture())?;
            let ciphertext_hash = match disposition {
                CaptureChunkDisposition::Accepted { ciphertext_hash }
                | CaptureChunkDisposition::Duplicate { ciphertext_hash } => ciphertext_hash,
            };
            prepared_artifact
                .sender
                .acknowledge(chunk.chunk_index, ciphertext_hash)
                .map_err(|_| invalid_capture())?;
            transfer_chunks = transfer_chunks.saturating_add(1);
        }
        let bytes = receiver.finish().map_err(|_| invalid_capture())?;
        transferred_bytes = transferred_bytes
            .checked_add(u64::try_from(bytes.len()).map_err(|_| invalid_capture())?)
            .ok_or_else(invalid_capture)?;
        prepared_artifact.artifact.bytes = bytes;
        prepared.bundle.artifacts.push(prepared_artifact.artifact);
    }
    let scene_hash = prepared.bundle.scene_hash.map(hash_hex);
    let evidence = PuppetCaptureEvidence {
        status: "complete".to_owned(),
        label: label.to_owned(),
        request_id: request.request_id.as_str().to_owned(),
        requester_device_id: request.requester_device_id.as_str().to_owned(),
        provider_device_id: request.provider_device_id.as_str().to_owned(),
        requested_revision: request.observed_revision,
        captured_revision: prepared.bundle.captured_revision,
        projection_hash: hash_hex(prepared.bundle.projection_hash),
        scene_hash,
        provider_kind: provider_kind.to_owned(),
        representation: representation.to_owned(),
        windowless,
        transferred_bytes,
        transfer_chunks,
        artifact_directory: String::new(),
        manifest_path: String::new(),
    };
    Ok(PendingPuppetCapture {
        bundle: prepared.bundle,
        evidence,
    })
}

fn hash_hex(hash: poche_protocol::SemanticHash) -> String {
    hash.0
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}

pub(crate) fn sign(key: &SigningKey, bytes: &[u8]) -> Result<SignatureBytes, DeviceClientError> {
    let signature = key.sign(bytes).to_bytes();
    let encoded = signature
        .iter()
        .fold(String::with_capacity(128), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        });
    SignatureBytes::new(encoded).map_err(|_| DeviceClientError::SigningFailed)
}

fn device_error(_: DeviceClientError) -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "certified native capture device rejected the puppet operation",
    )
}

const fn invalid_capture() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "puppet capture evidence violated its protocol binding",
    )
}

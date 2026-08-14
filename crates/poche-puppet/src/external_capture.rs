// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Private graphical capture over the external certified-device relay.

use std::{
    collections::BTreeSet,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::{Signer as _, SigningKey};
use poche_capture::{
    CaptureChunkFetchCall, CaptureChunkFetchResult, CaptureChunkUploadCall,
    CaptureDeliveryAcknowledgeCall, CaptureProvider, CaptureProviderPoll, CaptureProviderPollCall,
    CaptureProviderRegistrationCall, CaptureProviderResponseCall, CaptureProviderUnregisterCall,
    CaptureQualification, CaptureSurfaceMetadata, CaptureTransferReceiver, CaptureTransferSender,
    RawCaptureArtifact, RawCaptureBundle, capture_transfer_descriptor,
    generate_wrapped_capture_transfer_key, open_wrapped_capture_transfer_key,
};
use poche_native_ui::{
    NativeCaptureContext, NativeCaptureProvider, NativeLiveDevice, NativeUiLaunchOptions, run_live,
};
use poche_player_client::{
    DeviceClientError, DeviceCooperationRequest, DeviceProfile, DeviceSigner, HttpDeviceTransport,
    PlayerDeviceClient, sign_capture_provider_advertisement, sign_capture_request,
    sign_capture_response,
};
use poche_protocol::{
    CaptureArtifactDescriptorWire, CaptureConsentPolicyWire, CapturePrivacyWire,
    CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
    CaptureResponseOutcomeWire, CaptureTransferId, DEVICE_COOPERATION_SCHEMA_VERSION_V1,
    DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1, DeviceId, DeviceSignatureIntentWire,
    MAX_CAPTURE_ARTIFACT_BYTES, MAX_CAPTURE_TRANSFER_CHUNK_BYTES, PublicGameEventWire,
    PublicGamePhase, RoomId, RoomPhase, SemanticHash, SignatureAlgorithm, SignatureBytes,
    UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
    UnsignedCaptureResponseWire, capture_request_hash,
};

use crate::{
    PuppetError, PuppetErrorCode,
    browser::CertifiedBrowserRenderer,
    external::{
        external_alice_agent_identity, external_alice_browser_identity,
        external_alice_native_identity,
    },
    native::PendingPuppetCapture,
    scenario::{PuppetCaptureEvidence, hex},
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const REQUEST_LIFETIME_MS: u64 = 120_000;
const PROVIDER_LIFETIME_MS: u64 = 10 * 60 * 1_000;
const EXPECTED_CAPTURES: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalCaptureKind {
    Native,
    Browser,
}

impl ExternalCaptureKind {
    const fn wire(self) -> CaptureProviderKindWire {
        match self {
            Self::Native => CaptureProviderKindWire::NativeBevy,
            Self::Browser => CaptureProviderKindWire::BrowserHarness,
        }
    }

    fn representations(self) -> Vec<CaptureRepresentationWire> {
        match self {
            Self::Native => vec![CaptureRepresentationWire::Png],
            Self::Browser => vec![
                CaptureRepresentationWire::Png,
                CaptureRepresentationWire::SemanticHtml,
                CaptureRepresentationWire::AccessibilityTreeJson,
                CaptureRepresentationWire::LayoutJson,
            ],
        }
    }

    const fn provider_name(self) -> &'static str {
        match self {
            Self::Native => "native_bevy_external_relay",
            Self::Browser => "browser_harness_external_relay",
        }
    }

    const fn representation_name(self) -> &'static str {
        match self {
            Self::Native => "png",
            Self::Browser => "png,semantic_html,accessibility_tree_json,layout_json",
        }
    }

    const fn artifact_prefix(self) -> &'static str {
        match self {
            Self::Native => "external-native",
            Self::Browser => "external-browser",
        }
    }
}

#[derive(Clone)]
struct KeySigner(SigningKey);

impl DeviceSigner for KeySigner {
    fn sign_device_bytes(
        &self,
        _profile: &DeviceProfile,
        canonical_bytes: &[u8],
    ) -> Result<SignatureBytes, DeviceClientError> {
        SignatureBytes::new(hex(&self.0.sign(canonical_bytes).to_bytes()))
            .map_err(|_| DeviceClientError::SigningFailed)
    }
}

struct ProviderWorker {
    stop: mpsc::Sender<()>,
    join: Option<thread::JoinHandle<Result<usize, PuppetError>>>,
}

impl ProviderWorker {
    fn start(
        kind: ExternalCaptureKind,
        endpoint: &str,
        room_id: &RoomId,
        seed: u64,
    ) -> Result<Self, PuppetError> {
        let (profile, key) = provider_identity(kind, seed)?;
        let endpoint = endpoint.to_owned();
        let room_id = room_id.clone();
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name(format!("poche-{}-provider", kind.artifact_prefix()))
            .spawn(move || {
                provider_loop(
                    kind, seed, &endpoint, &room_id, &profile, &key, &stop_rx, &ready_tx,
                )
            })
            .map_err(|_| capture_transport_error())?;
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| capture_transport_error())??;
        Ok(Self {
            stop: stop_tx,
            join: Some(join),
        })
    }

    fn finish(mut self) -> Result<usize, PuppetError> {
        let _ = self.stop.send(());
        self.join
            .take()
            .ok_or_else(capture_protocol_error)?
            .join()
            .map_err(|_| capture_transport_error())?
    }
}

impl Drop for ProviderWorker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub(crate) struct ExternalRelayCaptureSession {
    kind: ExternalCaptureKind,
    worker: Option<ProviderWorker>,
    client: PlayerDeviceClient<HttpDeviceTransport<KeySigner>>,
    requester_profile: DeviceProfile,
    requester_key: SigningKey,
    provider_device_id: DeviceId,
    room_id: RoomId,
    seed: u64,
    captured: BTreeSet<&'static str>,
}

impl ExternalRelayCaptureSession {
    pub(crate) fn start(
        kind: ExternalCaptureKind,
        endpoint: &str,
        room_id: &RoomId,
        seed: u64,
    ) -> Result<Self, PuppetError> {
        let worker = ProviderWorker::start(kind, endpoint, room_id, seed)?;
        let (requester_profile, requester_key) = external_alice_agent_identity(seed)?;
        let (provider_profile, _) = provider_identity(kind, seed)?;
        let transport = HttpDeviceTransport::new(endpoint, 1, KeySigner(requester_key.clone()))
            .map_err(device_error)?;
        let client =
            PlayerDeviceClient::new(requester_profile.clone(), transport).map_err(device_error)?;
        Ok(Self {
            kind,
            worker: Some(worker),
            client,
            requester_profile,
            requester_key,
            provider_device_id: provider_profile.device_id,
            room_id: room_id.clone(),
            seed,
            captured: BTreeSet::new(),
        })
    }

    pub(crate) fn capture_next(&mut self) -> Result<Option<PendingPuppetCapture>, PuppetError> {
        let observation = self.client.observe(&self.room_id).map_err(device_error)?;
        let Some(label) = checkpoint(&observation) else {
            return Ok(None);
        };
        if !self.captured.insert(label) {
            return Ok(None);
        }
        let caption = checkpoint_caption(self.kind, label);
        self.capture(label, caption, &observation).map(Some)
    }

    pub(crate) fn finish(mut self) -> Result<(), PuppetError> {
        if self.captured.len() != EXPECTED_CAPTURES {
            return Err(PuppetError::new(
                PuppetErrorCode::DeviceProtocol,
                "external graphical provider omitted required semantic checkpoints",
            ));
        }
        let completed = self
            .worker
            .take()
            .ok_or_else(capture_protocol_error)?
            .finish()?;
        if completed != EXPECTED_CAPTURES {
            return Err(capture_protocol_error());
        }
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the requester keeps signed request construction, recipient-only key unwrap, bounded chunk verification, delivery acknowledgement, and evidence assembly in one auditable custody boundary"
    )]
    fn capture(
        &mut self,
        label: &'static str,
        caption: &'static str,
        observation: &poche_player_client::DeviceObservation,
    ) -> Result<PendingPuppetCapture, PuppetError> {
        let now = unix_time_ms()?;
        let revision = observation.projection.current_revision;
        let prefix = self.kind.artifact_prefix();
        let unsigned = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new(format!("{prefix}-{label}-{}-{revision}", self.seed))
                .map_err(|_| capture_protocol_error())?,
            room_id: self.room_id.clone(),
            membership_epoch: observation.projection.session_epoch,
            player_id: self.requester_profile.player_id.clone(),
            requester_device_id: self.requester_profile.device_id.clone(),
            provider_device_id: self.provider_device_id.clone(),
            observed_revision: revision,
            expires_at_unix_ms: now.saturating_add(REQUEST_LIFETIME_MS),
            replay_nonce: format!("{prefix}-{label}-nonce-{}-{revision}", self.seed),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: self.kind.wire(),
            representations: self.kind.representations(),
            viewport: None,
            label: caption.to_owned(),
            max_total_bytes: MAX_CAPTURE_ARTIFACT_BYTES,
            signature_intent: cooperation_intent(&self.requester_profile.device_id),
        };
        let request = sign_capture_request(
            &self.requester_profile,
            unsigned,
            &KeySigner(self.requester_key.clone()),
        )
        .map_err(device_error)?;
        let request_hash =
            capture_request_hash(&request.unsigned()).map_err(|_| capture_protocol_error())?;
        let receipt = self
            .client
            .transport_mut()
            .request_capture(
                &self.requester_profile,
                &self.provider_device_id,
                DeviceCooperationRequest::Capture(request.clone()),
            )
            .map_err(device_error)?;
        let artifacts = validate_response(&request, request_hash, &receipt.response)?;
        if artifacts.len() != receipt.wrapped_keys.len() || artifacts.is_empty() {
            return Err(capture_protocol_error());
        }
        let mut raw = Vec::with_capacity(artifacts.len());
        let mut transferred_bytes = 0_u64;
        let mut transfer_chunks = 0_u32;
        for (artifact, wrapped) in artifacts.iter().zip(&receipt.wrapped_keys) {
            if artifact.transfer.transfer_id != wrapped.transfer_id {
                return Err(capture_protocol_error());
            }
            let key = open_wrapped_capture_transfer_key(
                wrapped,
                &self.requester_profile.certificate,
                self.requester_key.as_bytes(),
            )
            .map_err(|_| capture_protocol_error())?;
            let mut receiver = CaptureTransferReceiver::new(
                artifact.transfer.clone(),
                request_hash,
                request.expires_at_unix_ms,
                key,
            )
            .map_err(|_| capture_protocol_error())?;
            for chunk_index in 0..artifact.transfer.chunk_count {
                loop {
                    match self
                        .client
                        .transport_mut()
                        .fetch_capture_chunk(&CaptureChunkFetchCall {
                            delivery_token: receipt.delivery_token.clone(),
                            request_id: request.request_id.clone(),
                            transfer_id: artifact.transfer.transfer_id.clone(),
                            chunk_index,
                        })
                        .map_err(device_error)?
                    {
                        CaptureChunkFetchResult::Pending => thread::sleep(POLL_INTERVAL),
                        CaptureChunkFetchResult::Available(chunk) => {
                            receiver
                                .accept(unix_time_ms()?, &chunk)
                                .map_err(|_| capture_protocol_error())?;
                            transfer_chunks = transfer_chunks.saturating_add(1);
                            break;
                        }
                    }
                }
            }
            let bytes = receiver.finish().map_err(|_| capture_protocol_error())?;
            transferred_bytes = transferred_bytes
                .checked_add(u64::try_from(bytes.len()).map_err(|_| capture_protocol_error())?)
                .ok_or_else(capture_protocol_error)?;
            raw.push(RawCaptureArtifact {
                artifact_id: artifact.artifact_id.clone(),
                representation: artifact.representation,
                media_type: media_type(artifact.representation).to_owned(),
                expected_source_hash: Some(artifact.transfer.content_hash),
                bytes,
            });
        }
        self.client
            .transport_mut()
            .acknowledge_capture_delivery(&CaptureDeliveryAcknowledgeCall {
                delivery_token: receipt.delivery_token,
                request_id: request.request_id.clone(),
            })
            .map_err(device_error)?;
        let first = artifacts.first().ok_or_else(capture_protocol_error)?;
        let viewport = first.viewport.ok_or_else(capture_protocol_error)?;
        let bundle = RawCaptureBundle {
            figure_id: format!("{prefix}-{label}-{}-{revision}", self.seed),
            caption: caption.to_owned(),
            captured_revision: first.captured_revision,
            projection_hash: first.projection_hash,
            scene_hash: first.scene_hash,
            surface: CaptureSurfaceMetadata {
                provider_kind: first.provider_kind,
                viewport,
                framebuffer_width: viewport.width_pixels,
                framebuffer_height: viewport.height_pixels,
                scale_milli: 1_000,
                camera: None,
            },
            qualification: match self.kind {
                ExternalCaptureKind::Native => CaptureQualification::RuntimeGenerated,
                ExternalCaptureKind::Browser => CaptureQualification::BrowserHarness,
            },
            cancelled: false,
            artifacts: raw,
        };
        Ok(PendingPuppetCapture {
            evidence: PuppetCaptureEvidence {
                status: "complete".to_owned(),
                label: label.to_owned(),
                request_id: request.request_id.as_str().to_owned(),
                requester_device_id: request.requester_device_id.as_str().to_owned(),
                provider_device_id: request.provider_device_id.as_str().to_owned(),
                requested_revision: request.observed_revision,
                captured_revision: bundle.captured_revision,
                projection_hash: hash_hex(bundle.projection_hash),
                scene_hash: bundle.scene_hash.map(hash_hex),
                provider_kind: self.kind.provider_name().to_owned(),
                representation: self.kind.representation_name().to_owned(),
                windowless: true,
                transferred_bytes,
                transfer_chunks,
                artifact_directory: String::new(),
                manifest_path: String::new(),
            },
            bundle,
        })
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the provider loop retains its exact socket, room, certified profile/key, lifecycle channels, and readiness result"
)]
fn provider_loop(
    kind: ExternalCaptureKind,
    seed: u64,
    endpoint: &str,
    room_id: &RoomId,
    profile: &DeviceProfile,
    key: &SigningKey,
    stop: &mpsc::Receiver<()>,
    ready: &mpsc::SyncSender<Result<(), PuppetError>>,
) -> Result<usize, PuppetError> {
    let signer = KeySigner(key.clone());
    let mut browser_renderer = match kind {
        ExternalCaptureKind::Native => None,
        ExternalCaptureKind::Browser => Some(CertifiedBrowserRenderer::new()?),
    };
    let transport = HttpDeviceTransport::new(endpoint, 1, signer.clone()).map_err(device_error)?;
    let mut client = PlayerDeviceClient::new(profile.clone(), transport).map_err(device_error)?;
    let observation = client.observe(room_id).map_err(device_error)?;
    let now = unix_time_ms()?;
    let advertisement = sign_capture_provider_advertisement(
        profile,
        UnsignedCaptureProviderAdvertisementWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            room_id: room_id.clone(),
            membership_epoch: observation.projection.session_epoch,
            player_id: profile.player_id.clone(),
            provider_device_id: profile.device_id.clone(),
            provider_kind: kind.wire(),
            representations: kind.representations(),
            privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
            consent_policy: CaptureConsentPolicyWire::HarnessOnly,
            max_total_bytes: MAX_CAPTURE_ARTIFACT_BYTES,
            advertisement_sequence: now.max(1),
            expires_at_unix_ms: now.saturating_add(PROVIDER_LIFETIME_MS),
            signature_intent: cooperation_intent(&profile.device_id),
        },
        &signer,
    )
    .map_err(device_error)?;
    let receipt = client
        .transport_mut()
        .register_capture_provider(&CaptureProviderRegistrationCall {
            certificate: profile.certificate.clone(),
            advertisement: advertisement.clone(),
        })
        .map_err(device_error)?;
    let _ = ready.send(Ok(()));
    let mut completed = 0_usize;
    let service = (|| -> Result<(), PuppetError> {
        loop {
            if stop.try_recv().is_ok() || completed >= EXPECTED_CAPTURES {
                return Ok(());
            }
            let job = client
                .transport_mut()
                .poll_capture_provider(&CaptureProviderPollCall {
                    provider_token: receipt.provider_token.clone(),
                })
                .map_err(device_error)?;
            let Some(job) = job else {
                thread::sleep(POLL_INTERVAL);
                continue;
            };
            match kind {
                ExternalCaptureKind::Native => serve_native_job(
                    endpoint,
                    room_id,
                    profile,
                    key,
                    &signer,
                    &advertisement,
                    &receipt.provider_token,
                    &mut client,
                    &job,
                )?,
                ExternalCaptureKind::Browser => serve_browser_job(
                    seed,
                    endpoint,
                    room_id,
                    profile,
                    key,
                    &signer,
                    &receipt.provider_token,
                    &mut client,
                    &job,
                    browser_renderer
                        .as_mut()
                        .ok_or_else(capture_protocol_error)?,
                )?,
            }
            completed = completed.saturating_add(1);
        }
    })();
    let unregister = client
        .transport_mut()
        .unregister_capture_provider(&CaptureProviderUnregisterCall {
            provider_token: receipt.provider_token,
        })
        .map_err(device_error);
    service?;
    unregister?;
    Ok(completed)
}

#[allow(
    clippy::too_many_arguments,
    reason = "one provider job binds separate render, signer, relay-token, exact profile, advertisement, and requester-certificate boundaries"
)]
fn serve_native_job(
    endpoint: &str,
    room_id: &RoomId,
    profile: &DeviceProfile,
    key: &SigningKey,
    signer: &KeySigner,
    advertisement: &poche_protocol::CaptureProviderAdvertisementWire,
    provider_token: &poche_capture::CaptureRelayToken,
    relay_client: &mut PlayerDeviceClient<HttpDeviceTransport<KeySigner>>,
    job: &poche_capture::CaptureRelayJob,
) -> Result<(), PuppetError> {
    if job.request.provider_device_id != profile.device_id || &job.request.room_id != room_id {
        return Err(capture_protocol_error());
    }
    let mut provider = NativeCaptureProvider::new(advertisement.clone());
    provider
        .begin_capture(job.request.clone())
        .map_err(|_| capture_protocol_error())?;
    let mut result = provider.clone();
    let native_transport =
        HttpDeviceTransport::new(endpoint, 1, KeySigner(key.clone())).map_err(device_error)?;
    let native_client =
        PlayerDeviceClient::new(profile.clone(), native_transport).map_err(device_error)?;
    let live = NativeLiveDevice::connect(native_client, room_id.clone())
        .map_err(|_| capture_transport_error())?;
    if live.observation().projection.current_revision != job.request.observed_revision {
        return Err(PuppetError::new(
            PuppetErrorCode::DeviceProtocol,
            "external native provider observed a different authority revision",
        ));
    }
    let projection_hash = live.observation().projection_hash;
    run_live(
        NativeUiLaunchOptions {
            exit_after_seconds: Some(3.0),
            hidden_window: true,
            capture_provider: Some(provider),
            capture_context: Some(NativeCaptureContext {
                current_revision: job.request.observed_revision,
                projection_hash,
            }),
            external_tracing: true,
            ..NativeUiLaunchOptions::default()
        },
        live,
    )
    .map_err(|_| capture_transport_error())?;
    let bundle = match result
        .poll_capture(&job.request.request_id)
        .map_err(|_| capture_protocol_error())?
    {
        CaptureProviderPoll::Ready(bundle) => *bundle,
        CaptureProviderPoll::Pending(_) | CaptureProviderPoll::Denied(_) => {
            return Err(capture_protocol_error());
        }
    };
    upload_bundle(
        signer,
        profile,
        provider_token,
        relay_client,
        &job.requester_certificate,
        &job.request,
        bundle,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "one browser provider job binds its renderer, exact HTTP device, signer, relay capability, request, and requester certificate"
)]
fn serve_browser_job(
    seed: u64,
    endpoint: &str,
    room_id: &RoomId,
    profile: &DeviceProfile,
    key: &SigningKey,
    signer: &KeySigner,
    provider_token: &poche_capture::CaptureRelayToken,
    relay_client: &mut PlayerDeviceClient<HttpDeviceTransport<KeySigner>>,
    job: &poche_capture::CaptureRelayJob,
    renderer: &mut CertifiedBrowserRenderer,
) -> Result<(), PuppetError> {
    if job.request.provider_device_id != profile.device_id
        || &job.request.room_id != room_id
        || job.request.provider_kind != CaptureProviderKindWire::BrowserHarness
        || job.request.representations != ExternalCaptureKind::Browser.representations()
    {
        return Err(capture_protocol_error());
    }
    let transport =
        HttpDeviceTransport::new(endpoint, 1, KeySigner(key.clone())).map_err(device_error)?;
    let mut browser_client =
        PlayerDeviceClient::new(profile.clone(), transport).map_err(device_error)?;
    let observation = browser_client.observe(room_id).map_err(device_error)?;
    if observation.projection.current_revision != job.request.observed_revision {
        return Err(PuppetError::new(
            PuppetErrorCode::DeviceProtocol,
            "external browser provider observed a different authority revision",
        ));
    }
    let label =
        request_checkpoint_label(&job.request.request_id).ok_or_else(capture_protocol_error)?;
    let document = poche_web_spike::certified_device_document(&observation, "Alice Browser")
        .map_err(|_| capture_protocol_error())?;
    let mut bundle = renderer.capture_document(
        seed,
        label,
        &document,
        job.request.observed_revision,
        observation.projection_hash,
    )?;
    job.request.label.clone_into(&mut bundle.caption);
    upload_bundle(
        signer,
        profile,
        provider_token,
        relay_client,
        &job.requester_certificate,
        &job.request,
        bundle,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "provider upload preserves the separate signer, profile, relay capability, requester certificate, signed request, and raw renderer bundle"
)]
fn upload_bundle(
    signer: &KeySigner,
    profile: &DeviceProfile,
    provider_token: &poche_capture::CaptureRelayToken,
    client: &mut PlayerDeviceClient<HttpDeviceTransport<KeySigner>>,
    requester_certificate: &poche_protocol::DeviceCertificateWire,
    request: &poche_protocol::CaptureRequestWire,
    mut bundle: RawCaptureBundle,
) -> Result<(), PuppetError> {
    let request_hash =
        capture_request_hash(&request.unsigned()).map_err(|_| capture_protocol_error())?;
    let mut prepared = Vec::new();
    let mut descriptors = Vec::new();
    for (index, mut artifact) in std::mem::take(&mut bundle.artifacts)
        .into_iter()
        .enumerate()
    {
        let transfer_id =
            CaptureTransferId::new(format!("{}-external-{index}", request.request_id.as_str()))
                .map_err(|_| capture_protocol_error())?;
        let descriptor = capture_transfer_descriptor(
            transfer_id.clone(),
            &artifact.bytes,
            MAX_CAPTURE_TRANSFER_CHUNK_BYTES,
        )
        .map_err(|_| capture_protocol_error())?;
        let (key, wrapped) =
            generate_wrapped_capture_transfer_key(requester_certificate, transfer_id, request_hash)
                .map_err(|_| capture_protocol_error())?;
        let sender = CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            request.expires_at_unix_ms,
            std::mem::take(&mut artifact.bytes),
            key,
            256,
        )
        .map_err(|_| capture_protocol_error())?;
        descriptors.push(CaptureArtifactDescriptorWire {
            artifact_id: artifact.artifact_id,
            representation: artifact.representation,
            provider_kind: bundle.surface.provider_kind,
            captured_revision: bundle.captured_revision,
            projection_hash: bundle.projection_hash,
            scene_hash: bundle.scene_hash,
            viewport: Some(bundle.surface.viewport),
            transfer: descriptor,
        });
        prepared.push((sender, wrapped));
    }
    let response = sign_capture_response(
        profile,
        UnsignedCaptureResponseWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash,
            room_id: request.room_id.clone(),
            membership_epoch: request.membership_epoch,
            player_id: request.player_id.clone(),
            requester_device_id: request.requester_device_id.clone(),
            provider_device_id: request.provider_device_id.clone(),
            outcome: CaptureResponseOutcomeWire::Accepted {
                artifacts: descriptors,
            },
            signature_intent: cooperation_intent(&profile.device_id),
        },
        signer,
    )
    .map_err(device_error)?;
    client
        .transport_mut()
        .provide_capture_response(&CaptureProviderResponseCall {
            provider_token: provider_token.clone(),
            response,
            wrapped_keys: prepared
                .iter()
                .map(|(_, wrapped)| wrapped.clone())
                .collect(),
        })
        .map_err(device_error)?;
    for (mut sender, _) in prepared {
        while let Some(chunk) = sender
            .next_chunk(unix_time_ms()?)
            .map_err(|_| capture_protocol_error())?
        {
            let index = chunk.chunk_index;
            let ciphertext_hash = SemanticHash(*blake3::hash(&chunk.ciphertext).as_bytes());
            client
                .transport_mut()
                .upload_capture_chunk(&CaptureChunkUploadCall {
                    provider_token: provider_token.clone(),
                    request_id: request.request_id.clone(),
                    chunk,
                })
                .map_err(device_error)?;
            sender
                .acknowledge(index, ciphertext_hash)
                .map_err(|_| capture_protocol_error())?;
        }
    }
    Ok(())
}

fn checkpoint(observation: &poche_player_client::DeviceObservation) -> Option<&'static str> {
    if observation.projection.payload.phase == RoomPhase::PostGame {
        return Some("terminal");
    }
    let game = observation.projection.payload.public_game_state.as_ref()?;
    if game.phase == PublicGamePhase::Bidding
        && game.round_index > 0
        && observation
            .projection
            .payload
            .public_history
            .iter()
            .any(|event| matches!(event, PublicGameEventWire::RoundScored { .. }))
    {
        return Some("score-sheet");
    }
    match game.phase {
        PublicGamePhase::Bidding => Some("bidding"),
        PublicGamePhase::Playing if !game.current_trick.is_empty() => Some("trick-in-progress"),
        PublicGamePhase::Playing if game.tricks_won.iter().any(|tricks| *tricks > 0) => {
            Some("trick-resolved")
        }
        PublicGamePhase::Playing => Some("card-selection"),
        PublicGamePhase::AwaitingDeal | PublicGamePhase::Scoring | PublicGamePhase::Finished => {
            None
        }
    }
}

fn checkpoint_caption(kind: ExternalCaptureKind, label: &str) -> &'static str {
    match (kind, label) {
        (ExternalCaptureKind::Native, "bidding") => "Bidding external native player view",
        (ExternalCaptureKind::Native, "card-selection") => {
            "Card selection external native player view"
        }
        (ExternalCaptureKind::Native, "trick-in-progress") => {
            "Trick in progress external native player view"
        }
        (ExternalCaptureKind::Native, "trick-resolved") => {
            "Resolved trick external native player view"
        }
        (ExternalCaptureKind::Native, "score-sheet") => "Score sheet external native player view",
        (ExternalCaptureKind::Native, "terminal") => "Terminal external native player view",
        (ExternalCaptureKind::Browser, "bidding") => "Bidding external browser player view",
        (ExternalCaptureKind::Browser, "card-selection") => {
            "Card selection external browser player view"
        }
        (ExternalCaptureKind::Browser, "trick-in-progress") => {
            "Trick in progress external browser player view"
        }
        (ExternalCaptureKind::Browser, "trick-resolved") => {
            "Resolved trick external browser player view"
        }
        (ExternalCaptureKind::Browser, "score-sheet") => "Score sheet external browser player view",
        (ExternalCaptureKind::Browser, "terminal") => "Terminal external browser player view",
        (ExternalCaptureKind::Native | ExternalCaptureKind::Browser, _) => {
            "External graphical player view"
        }
    }
}

fn request_checkpoint_label(request_id: &CaptureRequestId) -> Option<&'static str> {
    [
        "bidding",
        "card-selection",
        "trick-in-progress",
        "trick-resolved",
        "score-sheet",
        "terminal",
    ]
    .into_iter()
    .find(|label| request_id.as_str().contains(&format!("-{label}-")))
}

fn provider_identity(
    kind: ExternalCaptureKind,
    seed: u64,
) -> Result<(DeviceProfile, SigningKey), PuppetError> {
    match kind {
        ExternalCaptureKind::Native => external_alice_native_identity(seed),
        ExternalCaptureKind::Browser => external_alice_browser_identity(seed),
    }
}

fn validate_response<'a>(
    request: &poche_protocol::CaptureRequestWire,
    request_hash: SemanticHash,
    response: &'a poche_protocol::CaptureResponseWire,
) -> Result<&'a [CaptureArtifactDescriptorWire], PuppetError> {
    response.validate().map_err(|_| capture_protocol_error())?;
    if response.request_id != request.request_id
        || response.request_hash != request_hash
        || response.room_id != request.room_id
        || response.membership_epoch != request.membership_epoch
        || response.player_id != request.player_id
        || response.requester_device_id != request.requester_device_id
        || response.provider_device_id != request.provider_device_id
    {
        return Err(capture_protocol_error());
    }
    match &response.outcome {
        CaptureResponseOutcomeWire::Accepted { artifacts } => Ok(artifacts),
        CaptureResponseOutcomeWire::Denied { .. } => Err(PuppetError::new(
            PuppetErrorCode::ActionDenied,
            "external native provider denied an authorized capture request",
        )),
    }
}

fn cooperation_intent(device_id: &DeviceId) -> DeviceSignatureIntentWire {
    DeviceSignatureIntentWire {
        domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        algorithm: SignatureAlgorithm::Ed25519,
        key_id: device_id.clone(),
    }
}

fn media_type(representation: CaptureRepresentationWire) -> &'static str {
    match representation {
        CaptureRepresentationWire::Png => "image/png",
        CaptureRepresentationWire::SemanticHtml => "text/html; charset=utf-8",
        CaptureRepresentationWire::AccessibilityTreeJson
        | CaptureRepresentationWire::LayoutJson => "application/json",
    }
}

fn unix_time_ms() -> Result<u64, PuppetError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| capture_protocol_error())?
        .as_millis();
    u64::try_from(millis).map_err(|_| capture_protocol_error())
}

fn hash_hex(hash: SemanticHash) -> String {
    hex(&hash.0)
}

fn device_error(_: DeviceClientError) -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "external capture device rejected the protocol operation",
    )
}

const fn capture_protocol_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "external capture evidence violated its signed protocol binding",
    )
}

const fn capture_transport_error() -> PuppetError {
    PuppetError::new(
        PuppetErrorCode::DeviceProtocol,
        "external capture provider transport or renderer was unavailable",
    )
}

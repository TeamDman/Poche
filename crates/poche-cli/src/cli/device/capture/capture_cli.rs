use std::{
    path::PathBuf,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use eyre::{Context as _, Result, eyre};
use facet::Facet;
use figue as args;
use poche_capture::{
    CaptureChunkFetchCall, CaptureChunkFetchResult, CaptureChunkUploadCall,
    CaptureDeliveryAcknowledgeCall, CapturePipeline, CaptureProvider, CaptureProviderPoll,
    CaptureProviderPollCall, CaptureProviderRegistrationCall, CaptureProviderResponseCall,
    CaptureProviderUnregisterCall, CaptureQualification, CaptureSurfaceMetadata,
    CaptureTransferReceiver, CaptureTransferSender, RawCaptureArtifact, RawCaptureBundle,
    capture_transfer_descriptor, generate_wrapped_capture_transfer_key,
};
use poche_native_ui::{
    NativeCaptureContext, NativeCaptureProvider, NativeLiveDevice, NativeUiLaunchOptions, run_live,
};
use poche_player_client::{
    DeviceCooperationRequest, HttpDeviceTransport, PlayerDeviceClient, ProtectedProfileStore,
    sign_capture_provider_advertisement, sign_capture_request, sign_capture_response,
};
use poche_protocol::{
    CaptureArtifactDescriptorWire, CaptureConsentPolicyWire, CaptureDenialReasonWire,
    CapturePrivacyWire, CaptureProviderKindWire, CaptureRepresentationWire, CaptureRequestId,
    CaptureResponseOutcomeWire, CaptureTransferId, DEVICE_COOPERATION_SCHEMA_VERSION_V1,
    DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1, DeviceId, DeviceSignatureIntentWire,
    MAX_CAPTURE_ARTIFACT_BYTES, MAX_CAPTURE_TRANSFER_CHUNK_BYTES, SemanticHash, SignatureAlgorithm,
    UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
    UnsignedCaptureResponseWire, capture_request_hash,
};
use serde::Serialize;

use crate::cli::{
    live_device::LiveDeviceConfig,
    output::{OutputFormat, emit_value},
};

const REQUEST_LIFETIME_MS: u64 = 120_000;
const PROVIDER_LIFETIME_MS: u64 = 24 * 60 * 60 * 1_000;
const POLL_INTERVAL: Duration = Duration::from_millis(40);
const DEFAULT_CAPTURE_ROOT: &str = "target/poche-captures/live";

#[derive(Facet, PartialEq, Eq)]
pub struct CaptureArgs {
    #[facet(args::subcommand)]
    pub command: CaptureCommand,
}

/// Cross-device capture cooperation commands. `target_device` is exact, not a
/// display label or a local process selector.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum CaptureCommand {
    /// List signed same-player providers visible in this exact room/session.
    Providers {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
    },
    Request {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        target_device: String,
        #[facet(args::positional)]
        label: String,
        /// Requester-owned publication root. The renderer never sees it.
        #[facet(args::named)]
        output_dir: Option<String>,
    },
    /// Advertise this device's Bevy view and serve authorized requests.
    ServeNative {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
        /// Stop after the first completed or denied request.
        #[facet(args::named, default)]
        once: bool,
        /// Open an OS window for provider debugging; default is offscreen.
        #[facet(args::named, default)]
        show_window: bool,
    },
}

impl CaptureArgs {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self.command {
            CaptureCommand::Providers { .. } => "providers",
            CaptureCommand::Request { .. } => "request",
            CaptureCommand::ServeNative { .. } => "serve-native",
        }
    }

    #[must_use]
    pub const fn reports_cancellation_as_result(&self) -> bool {
        matches!(self.command, CaptureCommand::ServeNative { .. })
    }

    /// Execute a requester-owned capture publication or a windowless native
    /// provider worker through the certified HTTP cooperation lane.
    ///
    /// # Errors
    ///
    /// Fails closed on invalid profiles, signatures, authorization, relay
    /// responses, encrypted chunks, renderer evidence, or publication.
    pub fn invoke(
        self,
        config: &LiveDeviceConfig,
        output: OutputFormat,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<bool> {
        match self.command {
            CaptureCommand::Providers { profile, room } => {
                list_providers(config, &profile, &room, output)
            }
            CaptureCommand::Request {
                profile,
                room,
                target_device,
                label,
                output_dir,
            } => request_capture(
                config,
                &profile,
                &room,
                &target_device,
                &label,
                output_dir.as_deref(),
                output,
                &mut cancelled,
            ),
            CaptureCommand::ServeNative {
                profile,
                room,
                once,
                show_window,
            } => serve_native(
                config,
                &profile,
                &room,
                once,
                show_window,
                output,
                &mut cancelled,
            ),
        }
    }
}

fn list_providers(
    config: &LiveDeviceConfig,
    profile_label: &str,
    room: &str,
    output: OutputFormat,
) -> Result<bool> {
    let room_id = poche_protocol::RoomId::new(room.to_owned())
        .map_err(|_| eyre!("room identifier is invalid"))?;
    let mut client = config.client_for_profile(1, None, Some(profile_label))?;
    let observation = client.observe(&room_id)?;
    let text = if observation.capture_providers.is_empty() {
        "No same-player capture providers are currently advertised.".to_owned()
    } else {
        observation
            .capture_providers
            .iter()
            .map(|provider| {
                format!(
                    "{} — {:?} — {:?} — {:?}",
                    provider.provider_device_id.as_str(),
                    provider.provider_kind,
                    provider.representations,
                    provider.consent_policy
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    emit_value(&observation.capture_providers, &text, output)?;
    Ok(true)
}

#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "the CLI boundary keeps the signed request, recipient-only transfer verification, publication-before-ack, and each explicit user selector together for auditability"
)]
fn request_capture(
    config: &LiveDeviceConfig,
    profile_label: &str,
    room: &str,
    target_device: &str,
    label: &str,
    output_dir: Option<&str>,
    output: OutputFormat,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool> {
    let target_device =
        DeviceId::new(target_device.to_owned()).map_err(|_| eyre!("target device is invalid"))?;
    let store = ProtectedProfileStore::open_default()?;
    let profile = store.load_device(profile_label)?;
    let mut client = config.client_for_profile(1, None, Some(profile_label))?;
    let room_id = poche_protocol::RoomId::new(room.to_owned())
        .map_err(|_| eyre!("room identifier is invalid"))?;
    let observation = client.observe(&room_id)?;
    let now = unix_time_ms()?;
    let random = random_hex(16)?;
    let unsigned = UnsignedCaptureRequestWire {
        schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
        request_id: CaptureRequestId::new(format!("capture-{random}"))
            .map_err(|_| eyre!("capture request identifier is invalid"))?,
        room_id,
        membership_epoch: observation.projection.session_epoch,
        player_id: profile.player_id.clone(),
        requester_device_id: profile.device_id.clone(),
        provider_device_id: target_device.clone(),
        observed_revision: observation.projection.current_revision,
        expires_at_unix_ms: now.saturating_add(REQUEST_LIFETIME_MS),
        replay_nonce: format!("capture-nonce-{random}"),
        privacy: CapturePrivacyWire::ExactPlayerView,
        provider_kind: CaptureProviderKindWire::NativeBevy,
        representations: vec![CaptureRepresentationWire::Png],
        viewport: None,
        label: label.to_owned(),
        max_total_bytes: MAX_CAPTURE_ARTIFACT_BYTES,
        signature_intent: cooperation_intent(&profile.device_id),
    };
    let request = sign_capture_request(&profile, unsigned, &store)?;
    let request_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| eyre!("capture request is not canonical"))?;
    let receipt = client.transport_mut().request_capture(
        &profile,
        &target_device,
        DeviceCooperationRequest::Capture(request.clone()),
    )?;
    validate_response(&request, request_hash, &receipt.response)?;
    let CaptureResponseOutcomeWire::Accepted { artifacts } = &receipt.response.outcome else {
        let CaptureResponseOutcomeWire::Denied { reason } = receipt.response.outcome else {
            unreachable!();
        };
        return Err(eyre!("capture provider denied the request: {reason:?}"));
    };
    if artifacts.len() != receipt.wrapped_keys.len() {
        return Err(eyre!(
            "capture response key count does not match its artifacts"
        ));
    }

    let mut raw_artifacts = Vec::with_capacity(artifacts.len());
    for (artifact, wrapped) in artifacts.iter().zip(&receipt.wrapped_keys) {
        if artifact.transfer.transfer_id != wrapped.transfer_id {
            return Err(eyre!("capture response key names another transfer"));
        }
        let key = store.open_capture_transfer_key(&profile, wrapped)?;
        let mut receiver = CaptureTransferReceiver::new(
            artifact.transfer.clone(),
            request_hash,
            request.expires_at_unix_ms,
            key,
        )?;
        for chunk_index in 0..artifact.transfer.chunk_count {
            loop {
                if cancelled() {
                    receiver.cancel();
                    return Err(eyre!("capture request cancelled before publication"));
                }
                let fetched =
                    client
                        .transport_mut()
                        .fetch_capture_chunk(&CaptureChunkFetchCall {
                            delivery_token: receipt.delivery_token.clone(),
                            request_id: request.request_id.clone(),
                            transfer_id: artifact.transfer.transfer_id.clone(),
                            chunk_index,
                        })?;
                match fetched {
                    CaptureChunkFetchResult::Pending => thread::sleep(POLL_INTERVAL),
                    CaptureChunkFetchResult::Available(chunk) => {
                        receiver.accept(unix_time_ms()?, &chunk)?;
                        break;
                    }
                }
            }
        }
        raw_artifacts.push(RawCaptureArtifact {
            artifact_id: artifact.artifact_id.clone(),
            representation: artifact.representation,
            media_type: media_type(artifact.representation).to_owned(),
            bytes: receiver.finish()?,
            expected_source_hash: Some(artifact.transfer.content_hash),
        });
    }
    let first = artifacts
        .first()
        .ok_or_else(|| eyre!("accepted capture response contains no artifacts"))?;
    let viewport = first
        .viewport
        .ok_or_else(|| eyre!("PNG capture response omits its viewport"))?;
    let bundle = RawCaptureBundle {
        figure_id: format!("remote-{}", request.request_id.as_str()),
        caption: request.label.clone(),
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
        qualification: CaptureQualification::RuntimeGenerated,
        cancelled: false,
        artifacts: raw_artifacts,
    };
    let artifact_root = PathBuf::from(output_dir.unwrap_or(DEFAULT_CAPTURE_ROOT));
    let persisted = CapturePipeline::new(&artifact_root).persist(&bundle)?;
    client
        .transport_mut()
        .acknowledge_capture_delivery(&CaptureDeliveryAcknowledgeCall {
            delivery_token: receipt.delivery_token,
            request_id: request.request_id.clone(),
        })?;
    let summary = CaptureRequestSummary {
        schema: "poche.cli.capture-request.v1",
        status: "complete",
        request_id: request.request_id.as_str(),
        provider_device_id: target_device.as_str(),
        captured_revision: bundle.captured_revision,
        artifacts: persisted.manifest.entries.len(),
        directory: persisted.directory.to_string_lossy().into_owned(),
        manifest: persisted.manifest_path.to_string_lossy().into_owned(),
    };
    let text = format!(
        "capture {} complete at revision {}\nartifacts: {}\nmanifest: {}",
        summary.request_id, summary.captured_revision, summary.artifacts, summary.manifest
    );
    emit_value(&summary, &text, output)?;
    Ok(true)
}

fn serve_native(
    config: &LiveDeviceConfig,
    profile_label: &str,
    room: &str,
    once: bool,
    show_window: bool,
    output: OutputFormat,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool> {
    let store = ProtectedProfileStore::open_default()?;
    let profile = store.load_device(profile_label)?;
    let room_id = poche_protocol::RoomId::new(room.to_owned())
        .map_err(|_| eyre!("room identifier is invalid"))?;
    let mut registration_client = config.client_for_profile(1, None, Some(profile_label))?;
    let observation = registration_client.observe(&room_id)?;
    let advertisement = sign_capture_provider_advertisement(
        &profile,
        UnsignedCaptureProviderAdvertisementWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            room_id: room_id.clone(),
            membership_epoch: observation.projection.session_epoch,
            player_id: profile.player_id.clone(),
            provider_device_id: profile.device_id.clone(),
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
            consent_policy: CaptureConsentPolicyWire::Automatic,
            max_total_bytes: MAX_CAPTURE_ARTIFACT_BYTES,
            advertisement_sequence: unix_time_ms()?.max(1),
            expires_at_unix_ms: unix_time_ms()?.saturating_add(PROVIDER_LIFETIME_MS),
            signature_intent: cooperation_intent(&profile.device_id),
        },
        &store,
    )?;
    let receipt = registration_client
        .transport_mut()
        .register_capture_provider(&CaptureProviderRegistrationCall {
            certificate: profile.certificate.clone(),
            advertisement: advertisement.clone(),
        })?;
    let mut completed = 0_u64;
    let mut denied = 0_u64;
    let service_result = (|| -> Result<()> {
        while !cancelled() {
            let job = registration_client.transport_mut().poll_capture_provider(
                &CaptureProviderPollCall {
                    provider_token: receipt.provider_token.clone(),
                },
            )?;
            let Some(job) = job else {
                thread::sleep(POLL_INTERVAL);
                continue;
            };
            if serve_native_job(
                config,
                profile_label,
                &store,
                &profile,
                &advertisement,
                &receipt.provider_token,
                &mut registration_client,
                job,
                show_window,
            )? {
                completed = completed.saturating_add(1);
            } else {
                denied = denied.saturating_add(1);
            }
            if once {
                break;
            }
        }
        Ok(())
    })();
    let unregister_result = registration_client
        .transport_mut()
        .unregister_capture_provider(&CaptureProviderUnregisterCall {
            provider_token: receipt.provider_token,
        });
    service_result?;
    unregister_result?;
    let handled_request = completed.saturating_add(denied) > 0;
    let summary = CaptureProviderSummary {
        schema: "poche.cli.capture-provider.v1",
        status: if once && handled_request {
            "one-request-complete"
        } else {
            "cancelled"
        },
        profile: profile_label,
        room,
        provider_device_id: profile.device_id.as_str(),
        completed,
        denied,
        windowless: !show_window,
    };
    let text = format!(
        "native capture provider stopped ({} complete, {} denied, windowless={})",
        summary.completed, summary.denied, summary.windowless
    );
    emit_value(&summary, &text, output)?;
    Ok(true)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the provider job names its separate protected signer, relay transport, render adapter, and exact authorization records"
)]
fn serve_native_job(
    config: &LiveDeviceConfig,
    profile_label: &str,
    store: &ProtectedProfileStore,
    profile: &poche_player_client::DeviceProfile,
    advertisement: &poche_protocol::CaptureProviderAdvertisementWire,
    provider_token: &poche_capture::CaptureRelayToken,
    relay_client: &mut PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>,
    job: poche_capture::CaptureRelayJob,
    show_window: bool,
) -> Result<bool> {
    let request = job.request;
    if request.provider_device_id != profile.device_id {
        return Err(eyre!("capture relay delivered a job for another device"));
    }
    let mut provider = NativeCaptureProvider::new(advertisement.clone());
    if provider.begin_capture(request.clone()).is_err() {
        send_denial(
            store,
            profile,
            provider_token,
            relay_client,
            &request,
            CaptureDenialReasonWire::UnsupportedFormat,
        )?;
        return Ok(false);
    }
    let mut result = provider.clone();
    let native_client = config.client_for_profile(1, None, Some(profile_label))?;
    let live = NativeLiveDevice::connect(native_client, request.room_id.clone())
        .map_err(|error| eyre!(error))?;
    if live.observation().projection.current_revision != request.observed_revision {
        send_denial(
            store,
            profile,
            provider_token,
            relay_client,
            &request,
            CaptureDenialReasonWire::StaleRevision,
        )?;
        return Ok(false);
    }
    let projection_hash = live.observation().projection_hash;
    if let Err(error) = run_live(
        NativeUiLaunchOptions {
            exit_after_seconds: Some(3.0),
            hidden_window: !show_window,
            capture_provider: Some(provider),
            capture_context: Some(NativeCaptureContext {
                current_revision: request.observed_revision,
                projection_hash,
            }),
            external_tracing: true,
            ..NativeUiLaunchOptions::default()
        },
        live,
    ) {
        send_denial(
            store,
            profile,
            provider_token,
            relay_client,
            &request,
            CaptureDenialReasonWire::ProviderUnavailable,
        )?;
        return Err(eyre!("native capture renderer failed: {error}"));
    }
    let bundle = match result.poll_capture(&request.request_id)? {
        CaptureProviderPoll::Ready(bundle) => *bundle,
        CaptureProviderPoll::Denied(reason) => {
            send_denial(
                store,
                profile,
                provider_token,
                relay_client,
                &request,
                reason,
            )?;
            return Ok(false);
        }
        CaptureProviderPoll::Pending(_) => {
            send_denial(
                store,
                profile,
                provider_token,
                relay_client,
                &request,
                CaptureDenialReasonWire::ProviderUnavailable,
            )?;
            return Ok(false);
        }
    };
    upload_bundle(
        store,
        profile,
        provider_token,
        relay_client,
        &job.requester_certificate,
        &request,
        bundle,
    )?;
    Ok(true)
}

fn upload_bundle(
    store: &ProtectedProfileStore,
    profile: &poche_player_client::DeviceProfile,
    provider_token: &poche_capture::CaptureRelayToken,
    relay_client: &mut PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>,
    requester_certificate: &poche_protocol::DeviceCertificateWire,
    request: &poche_protocol::CaptureRequestWire,
    mut bundle: RawCaptureBundle,
) -> Result<()> {
    let request_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| eyre!("capture request is not canonical"))?;
    let mut prepared = Vec::new();
    let mut descriptors = Vec::new();
    for (index, mut artifact) in std::mem::take(&mut bundle.artifacts)
        .into_iter()
        .enumerate()
    {
        let transfer_id =
            CaptureTransferId::new(format!("{}-{index}", request.request_id.as_str()))
                .map_err(|_| eyre!("capture transfer identifier is invalid"))?;
        let descriptor = capture_transfer_descriptor(
            transfer_id.clone(),
            &artifact.bytes,
            MAX_CAPTURE_TRANSFER_CHUNK_BYTES,
        )?;
        let (key, wrapped) = generate_wrapped_capture_transfer_key(
            requester_certificate,
            transfer_id,
            request_hash,
        )?;
        let sender = CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            request.expires_at_unix_ms,
            std::mem::take(&mut artifact.bytes),
            key,
            1,
        )?;
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
        store,
    )?;
    relay_client
        .transport_mut()
        .provide_capture_response(&CaptureProviderResponseCall {
            provider_token: provider_token.clone(),
            response,
            wrapped_keys: prepared
                .iter()
                .map(|(_, wrapped)| wrapped.clone())
                .collect(),
        })?;
    for (mut sender, _) in prepared {
        while let Some(chunk) = sender.next_chunk(unix_time_ms()?)? {
            let index = chunk.chunk_index;
            let ciphertext_hash = SemanticHash(*blake3::hash(&chunk.ciphertext).as_bytes());
            relay_client
                .transport_mut()
                .upload_capture_chunk(&CaptureChunkUploadCall {
                    provider_token: provider_token.clone(),
                    request_id: request.request_id.clone(),
                    chunk,
                })?;
            sender.acknowledge(index, ciphertext_hash)?;
        }
    }
    Ok(())
}

fn send_denial(
    store: &ProtectedProfileStore,
    profile: &poche_player_client::DeviceProfile,
    provider_token: &poche_capture::CaptureRelayToken,
    relay_client: &mut PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>,
    request: &poche_protocol::CaptureRequestWire,
    reason: CaptureDenialReasonWire,
) -> Result<()> {
    let request_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| eyre!("capture request is not canonical"))?;
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
            outcome: CaptureResponseOutcomeWire::Denied { reason },
            signature_intent: cooperation_intent(&profile.device_id),
        },
        store,
    )?;
    relay_client
        .transport_mut()
        .provide_capture_response(&CaptureProviderResponseCall {
            provider_token: provider_token.clone(),
            response,
            wrapped_keys: Vec::new(),
        })?;
    Ok(())
}

fn validate_response(
    request: &poche_protocol::CaptureRequestWire,
    request_hash: SemanticHash,
    response: &poche_protocol::CaptureResponseWire,
) -> Result<()> {
    response
        .validate()
        .map_err(|_| eyre!("capture provider response is invalid"))?;
    if response.request_id != request.request_id
        || response.request_hash != request_hash
        || response.room_id != request.room_id
        || response.membership_epoch != request.membership_epoch
        || response.player_id != request.player_id
        || response.requester_device_id != request.requester_device_id
        || response.provider_device_id != request.provider_device_id
    {
        Err(eyre!(
            "capture provider response does not match the request"
        ))
    } else {
        Ok(())
    }
}

fn cooperation_intent(device_id: &DeviceId) -> DeviceSignatureIntentWire {
    DeviceSignatureIntentWire {
        domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        algorithm: SignatureAlgorithm::Ed25519,
        key_id: device_id.clone(),
    }
}

const fn media_type(representation: CaptureRepresentationWire) -> &'static str {
    match representation {
        CaptureRepresentationWire::Png => "image/png",
        CaptureRepresentationWire::SemanticHtml => "text/html",
        CaptureRepresentationWire::AccessibilityTreeJson
        | CaptureRepresentationWire::LayoutJson => "application/json",
    }
}

fn unix_time_ms() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .wrap_err("system clock is before the Unix epoch")?
            .as_millis(),
    )
    .wrap_err("system clock does not fit the capture protocol")
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random).wrap_err("operating-system randomness is unavailable")?;
    Ok(random.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    }))
}

#[derive(Serialize)]
struct CaptureRequestSummary<'a> {
    schema: &'static str,
    status: &'static str,
    request_id: &'a str,
    provider_device_id: &'a str,
    captured_revision: u64,
    artifacts: usize,
    directory: String,
    manifest: String,
}

#[derive(Serialize)]
struct CaptureProviderSummary<'a> {
    schema: &'static str,
    status: &'static str,
    profile: &'a str,
    room: &'a str,
    provider_device_id: &'a str,
    completed: u64,
    denied: u64,
    windowless: bool,
}

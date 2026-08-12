use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use bevy::prelude::Resource;
use poche_capture::{CapturePipelineError, CaptureProvider, CaptureProviderPoll, RawCaptureBundle};
use poche_protocol::{
    CaptureConsentPolicyWire, CaptureDenialReasonWire, CaptureProgressStageWire,
    CaptureProviderAdvertisementWire, CaptureProviderKindWire, CaptureRepresentationWire,
    CaptureRequestId, CaptureRequestWire, SemanticHash,
};

/// Exact live projection identity paired with the Bevy leaf adapter.
#[derive(Clone, Copy, Debug, Resource)]
pub struct NativeCaptureContext {
    pub current_revision: u64,
    pub projection_hash: SemanticHash,
}

#[derive(Clone, Debug)]
enum NativeCaptureState {
    AwaitingConsent(CaptureRequestWire),
    Queued(CaptureRequestWire),
    Capturing,
    Ready(RawCaptureBundle),
    Denied(CaptureDenialReasonWire),
}

#[derive(Debug, Default)]
struct NativeCaptureShared {
    jobs: BTreeMap<CaptureRequestId, NativeCaptureState>,
}

/// Nonblocking Bevy capture provider handle shared with the render world.
/// It returns raw bundles and never owns evidence paths.
#[derive(Clone, Debug, Resource)]
pub struct NativeCaptureProvider {
    advertisement: CaptureProviderAdvertisementWire,
    shared: Arc<Mutex<NativeCaptureShared>>,
}

impl NativeCaptureProvider {
    #[must_use]
    pub fn new(advertisement: CaptureProviderAdvertisementWire) -> Self {
        Self {
            advertisement,
            shared: Arc::new(Mutex::new(NativeCaptureShared::default())),
        }
    }

    /// Resolve one pending local confirmation without blocking the render loop.
    ///
    /// # Errors
    ///
    /// Returns a stable pipeline error when the request is unknown, no longer
    /// awaiting consent, or the shared provider state is unavailable.
    pub fn decide_consent(
        &self,
        request_id: &CaptureRequestId,
        granted: bool,
    ) -> Result<(), CapturePipelineError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        let state = shared
            .jobs
            .get_mut(request_id)
            .ok_or(CapturePipelineError::InvalidMetadata)?;
        let NativeCaptureState::AwaitingConsent(request) = state else {
            return Err(CapturePipelineError::InvalidMetadata);
        };
        *state = if granted {
            NativeCaptureState::Queued(request.clone())
        } else {
            NativeCaptureState::Denied(CaptureDenialReasonWire::ConsentDenied)
        };
        Ok(())
    }

    /// Take one queued request at a Bevy update boundary.
    pub(crate) fn take_queued(&self) -> Result<Option<CaptureRequestWire>, CapturePipelineError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        let request_id = shared.jobs.iter().find_map(|(request_id, state)| {
            matches!(state, NativeCaptureState::Queued(_)).then(|| request_id.clone())
        });
        let Some(request_id) = request_id else {
            return Ok(None);
        };
        let state = shared
            .jobs
            .get_mut(&request_id)
            .ok_or(CapturePipelineError::InvalidMetadata)?;
        let NativeCaptureState::Queued(request) = state else {
            return Err(CapturePipelineError::InvalidMetadata);
        };
        let request = request.clone();
        *state = NativeCaptureState::Capturing;
        Ok(Some(request))
    }

    pub(crate) fn complete(
        &self,
        request_id: &CaptureRequestId,
        bundle: RawCaptureBundle,
    ) -> Result<(), CapturePipelineError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        let state = shared
            .jobs
            .get_mut(request_id)
            .ok_or(CapturePipelineError::InvalidMetadata)?;
        if !matches!(state, NativeCaptureState::Capturing) {
            return Err(CapturePipelineError::InvalidMetadata);
        }
        *state = NativeCaptureState::Ready(bundle);
        Ok(())
    }

    pub(crate) fn deny(
        &self,
        request_id: &CaptureRequestId,
        reason: CaptureDenialReasonWire,
    ) -> Result<(), CapturePipelineError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        let state = shared
            .jobs
            .get_mut(request_id)
            .ok_or(CapturePipelineError::InvalidMetadata)?;
        *state = NativeCaptureState::Denied(reason);
        Ok(())
    }
}

impl CaptureProvider for NativeCaptureProvider {
    fn advertisement(&self) -> &CaptureProviderAdvertisementWire {
        &self.advertisement
    }

    fn begin_capture(&mut self, request: CaptureRequestWire) -> Result<(), CapturePipelineError> {
        request
            .validate()
            .map_err(|_| CapturePipelineError::InvalidMetadata)?;
        if request.provider_device_id != self.advertisement.provider_device_id
            || request.player_id != self.advertisement.player_id
            || request.room_id != self.advertisement.room_id
            || request.membership_epoch != self.advertisement.membership_epoch
            || request.provider_kind != CaptureProviderKindWire::NativeBevy
            || request.representations != vec![CaptureRepresentationWire::Png]
        {
            return Err(CapturePipelineError::InvalidMetadata);
        }
        let state = match self.advertisement.consent_policy {
            CaptureConsentPolicyWire::Automatic | CaptureConsentPolicyWire::HarnessOnly => {
                NativeCaptureState::Queued(request.clone())
            }
            CaptureConsentPolicyWire::UserConfirmation => {
                NativeCaptureState::AwaitingConsent(request.clone())
            }
        };
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        if shared.jobs.contains_key(&request.request_id) {
            return Err(CapturePipelineError::DuplicateId);
        }
        shared.jobs.insert(request.request_id, state);
        Ok(())
    }

    fn poll_capture(
        &mut self,
        request_id: &CaptureRequestId,
    ) -> Result<CaptureProviderPoll, CapturePipelineError> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| CapturePipelineError::Storage)?;
        let state = shared
            .jobs
            .get(request_id)
            .ok_or(CapturePipelineError::InvalidMetadata)?;
        let poll = match state {
            NativeCaptureState::AwaitingConsent(_) => {
                CaptureProviderPoll::Pending(CaptureProgressStageWire::AwaitingConsent)
            }
            NativeCaptureState::Queued(_) | NativeCaptureState::Capturing => {
                CaptureProviderPoll::Pending(CaptureProgressStageWire::Capturing)
            }
            NativeCaptureState::Denied(reason) => CaptureProviderPoll::Denied(*reason),
            NativeCaptureState::Ready(_) => {
                let NativeCaptureState::Ready(bundle) = shared
                    .jobs
                    .remove(request_id)
                    .ok_or(CapturePipelineError::InvalidMetadata)?
                else {
                    return Err(CapturePipelineError::InvalidMetadata);
                };
                CaptureProviderPoll::Ready(Box::new(bundle))
            }
        };
        Ok(poll)
    }

    fn cancel_capture(
        &mut self,
        request_id: &CaptureRequestId,
    ) -> Result<(), CapturePipelineError> {
        self.deny(request_id, CaptureDenialReasonWire::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use poche_capture::{
        CaptureQualification, CaptureSurfaceMetadata, RawCaptureArtifact, RawCaptureBundle,
    };
    use poche_protocol::{
        CaptureArtifactId, CapturePrivacyWire, CaptureViewportWire,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1, DeviceId,
        DeviceSignatureIntentWire, PrincipalId, RoomId, SignatureAlgorithm, SignatureBytes,
        UnsignedCaptureProviderAdvertisementWire, UnsignedCaptureRequestWire,
    };

    use super::*;

    fn signature() -> SignatureBytes {
        SignatureBytes::new("11".repeat(64)).expect("structural signature")
    }

    fn fixture(policy: CaptureConsentPolicyWire) -> (NativeCaptureProvider, CaptureRequestWire) {
        let room_id = RoomId::new("room-native-capture").expect("room");
        let player_id = PrincipalId::new("player-alice").expect("player");
        let requester_device_id = DeviceId::new("device-cli").expect("requester device");
        let provider_device_id = DeviceId::new("device-bevy").expect("provider device");
        let provider_intent = DeviceSignatureIntentWire {
            domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: provider_device_id.clone(),
        };
        let advertisement = UnsignedCaptureProviderAdvertisementWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            room_id: room_id.clone(),
            membership_epoch: 7,
            player_id: player_id.clone(),
            provider_device_id: provider_device_id.clone(),
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
            consent_policy: policy,
            max_total_bytes: 8 * 1024 * 1024,
            advertisement_sequence: 1,
            expires_at_unix_ms: 2_000_000_000_000,
            signature_intent: provider_intent,
        }
        .attach_signature(signature())
        .expect("advertisement");
        let request = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new("capture-native-1").expect("request id"),
            room_id,
            membership_epoch: 7,
            player_id,
            requester_device_id: requester_device_id.clone(),
            provider_device_id,
            observed_revision: 42,
            expires_at_unix_ms: 2_000_000_000_000,
            replay_nonce: "native-capture-nonce-1".to_owned(),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            viewport: Some(CaptureViewportWire {
                width_pixels: 1280,
                height_pixels: 800,
            }),
            label: "native bidding view".to_owned(),
            max_total_bytes: 8 * 1024 * 1024,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: requester_device_id,
            },
        }
        .attach_signature(signature())
        .expect("request");
        (NativeCaptureProvider::new(advertisement), request)
    }

    fn raw_bundle(request: &CaptureRequestWire) -> RawCaptureBundle {
        RawCaptureBundle {
            figure_id: "native-bidding".to_owned(),
            caption: "Native bidding view".to_owned(),
            captured_revision: request.observed_revision,
            projection_hash: SemanticHash([3; 32]),
            scene_hash: Some(SemanticHash([4; 32])),
            surface: CaptureSurfaceMetadata {
                provider_kind: CaptureProviderKindWire::NativeBevy,
                viewport: request.viewport.expect("viewport"),
                framebuffer_width: 1280,
                framebuffer_height: 800,
                scale_milli: 1000,
                camera: None,
            },
            qualification: CaptureQualification::RuntimeGenerated,
            cancelled: false,
            artifacts: vec![RawCaptureArtifact {
                artifact_id: CaptureArtifactId::new("native-png").expect("artifact id"),
                representation: CaptureRepresentationWire::Png,
                media_type: "image/png".to_owned(),
                bytes: vec![1, 2, 3],
                expected_source_hash: None,
            }],
        }
    }

    #[test]
    fn user_consent_queues_without_blocking_and_returns_raw_bundle() {
        let (mut provider, request) = fixture(CaptureConsentPolicyWire::UserConfirmation);
        provider
            .begin_capture(request.clone())
            .expect("begin capture");
        assert_eq!(
            provider.poll_capture(&request.request_id),
            Ok(CaptureProviderPoll::Pending(
                CaptureProgressStageWire::AwaitingConsent
            ))
        );

        provider
            .decide_consent(&request.request_id, true)
            .expect("grant consent");
        assert_eq!(provider.take_queued(), Ok(Some(request.clone())));
        provider
            .complete(&request.request_id, raw_bundle(&request))
            .expect("complete capture");
        assert!(matches!(
            provider.poll_capture(&request.request_id),
            Ok(CaptureProviderPoll::Ready(_))
        ));
    }

    #[test]
    fn cancellation_and_duplicate_ids_are_explicit() {
        let (mut provider, request) = fixture(CaptureConsentPolicyWire::Automatic);
        provider
            .begin_capture(request.clone())
            .expect("begin capture");
        assert_eq!(
            provider.begin_capture(request.clone()),
            Err(CapturePipelineError::DuplicateId)
        );
        assert_eq!(provider.take_queued(), Ok(Some(request.clone())));
        provider
            .cancel_capture(&request.request_id)
            .expect("cancel capture");
        assert_eq!(
            provider.poll_capture(&request.request_id),
            Ok(CaptureProviderPoll::Denied(
                CaptureDenialReasonWire::Cancelled
            ))
        );
    }
}

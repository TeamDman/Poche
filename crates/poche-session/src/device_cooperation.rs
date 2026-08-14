// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pure authorization for non-authoritative cooperation between devices.

#![allow(
    clippy::missing_errors_doc,
    reason = "both public authorization functions return the documented stable redacted CaptureAuthorizationError categories"
)]

use core::fmt;
use std::collections::BTreeMap;

use poche_protocol::{
    CaptureCompletionOutcomeWire, CaptureCompletionWire, CaptureConsentWire, CaptureProgressWire,
    CaptureProviderAdvertisementWire, CaptureRequestId, CaptureRequestWire,
    CaptureResponseOutcomeWire, CaptureResponseWire, DeviceCapabilityWire, DeviceCertificateWire,
    DeviceId, PrincipalId, RoomId, SemanticHash, capture_request_hash,
};

/// Exact current-room context used to authorize a capture exchange.
///
/// Signature verification remains the transport adapter's responsibility.
/// The booleans make revocation evidence explicit rather than assuming that a
/// structurally valid old certificate is still active.
pub struct CaptureAuthorizationContext<'a> {
    pub room_id: &'a RoomId,
    pub membership_epoch: u64,
    pub current_revision: u64,
    pub now_unix_ms: u64,
    pub requester_certificate: &'a DeviceCertificateWire,
    pub provider_certificate: &'a DeviceCertificateWire,
    pub requester_revoked: bool,
    pub provider_revoked: bool,
}

/// Current-room context for one provider advertisement.
pub struct CaptureProviderAuthorizationContext<'a> {
    pub room_id: &'a RoomId,
    pub membership_epoch: u64,
    pub now_unix_ms: u64,
    pub provider_certificate: &'a DeviceCertificateWire,
    pub provider_revoked: bool,
}

/// Redacted failure from exact-target capture authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureAuthorizationError {
    InvalidWire,
    WrongRoom,
    WrongMembershipEpoch,
    WrongPlayer,
    WrongDevice,
    RevokedDevice,
    ExpiredCertificate,
    MissingCapability,
    ExpiredRequest,
    StaleRevision,
    ResponseMismatch,
    ArtifactMismatch,
    AdvertisementMismatch,
    ProgressMismatch,
    CompletionMismatch,
    ReplayedRequest,
    ConflictingReplay,
    ReplayWindowFull,
}

impl fmt::Display for CaptureAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidWire => "device-cooperation wire value is invalid",
            Self::WrongRoom => "capture request targets another room",
            Self::WrongMembershipEpoch => "capture request targets another membership epoch",
            Self::WrongPlayer => "capture devices do not share the certified player root",
            Self::WrongDevice => "capture request targets different device certificates",
            Self::RevokedDevice => "capture device certificate is revoked",
            Self::ExpiredCertificate => "capture device certificate is not active in this epoch",
            Self::MissingCapability => "capture device lacks the required capability",
            Self::ExpiredRequest => "capture request has expired",
            Self::StaleRevision => "capture request does not bind the current projection revision",
            Self::ResponseMismatch => "capture response does not bind the authorized request",
            Self::ArtifactMismatch => "capture artifacts violate the authorized request",
            Self::AdvertisementMismatch => "capture request exceeds the provider advertisement",
            Self::ProgressMismatch => "capture progress does not bind the authorized request",
            Self::CompletionMismatch => "capture completion does not match the artifact offer",
            Self::ReplayedRequest => "capture request was already accepted",
            Self::ConflictingReplay => "capture request id was reused with different content",
            Self::ReplayWindowFull => "capture replay window is full",
        })
    }
}

impl std::error::Error for CaptureAuthorizationError {}

/// Bounded non-authoritative replay memory for accepted capture requests.
/// This state belongs to a device-cooperation adapter, never `SessionState`.
pub struct CaptureReplayWindow {
    capacity: usize,
    accepted: BTreeMap<CaptureRequestId, SemanticHash>,
}

impl CaptureReplayWindow {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            accepted: BTreeMap::new(),
        }
    }

    pub fn authorize_once(
        &mut self,
        request: &CaptureRequestWire,
        context: &CaptureAuthorizationContext<'_>,
    ) -> Result<(), CaptureAuthorizationError> {
        authorize_capture_request(request, context)?;
        let hash = capture_request_hash(&request.unsigned())
            .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
        if let Some(accepted) = self.accepted.get(&request.request_id) {
            return Err(if *accepted == hash {
                CaptureAuthorizationError::ReplayedRequest
            } else {
                CaptureAuthorizationError::ConflictingReplay
            });
        }
        if self.accepted.len() >= self.capacity {
            return Err(CaptureAuthorizationError::ReplayWindowFull);
        }
        self.accepted.insert(request.request_id.clone(), hash);
        Ok(())
    }
}

/// Authorize one signed provider advertisement after its device signature has
/// been verified by the adapter.
pub fn authorize_capture_provider(
    advertisement: &CaptureProviderAdvertisementWire,
    context: &CaptureProviderAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    advertisement
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    context
        .provider_certificate
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    if &advertisement.room_id != context.room_id {
        return Err(CaptureAuthorizationError::WrongRoom);
    }
    if advertisement.membership_epoch != context.membership_epoch {
        return Err(CaptureAuthorizationError::WrongMembershipEpoch);
    }
    if advertisement.player_id != context.provider_certificate.player_id {
        return Err(CaptureAuthorizationError::WrongPlayer);
    }
    if advertisement.provider_device_id != context.provider_certificate.device_id {
        return Err(CaptureAuthorizationError::WrongDevice);
    }
    if context.provider_revoked {
        return Err(CaptureAuthorizationError::RevokedDevice);
    }
    if !context
        .provider_certificate
        .is_valid_at(context.membership_epoch)
    {
        return Err(CaptureAuthorizationError::ExpiredCertificate);
    }
    if !context
        .provider_certificate
        .has_capability(DeviceCapabilityWire::ProvideCapture)
    {
        return Err(CaptureAuthorizationError::MissingCapability);
    }
    if advertisement.expires_at_unix_ms < context.now_unix_ms {
        return Err(CaptureAuthorizationError::ExpiredRequest);
    }
    Ok(())
}

/// Authorize an exact-target request after the adapter has verified the
/// requester signature against `requester_certificate`.
///
/// This grants permission to ask the provider. Interactive consent or a local
/// provider policy may still deny the request without changing game state.
pub fn authorize_capture_request(
    request: &CaptureRequestWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    request
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    context
        .requester_certificate
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    context
        .provider_certificate
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;

    if &request.room_id != context.room_id {
        return Err(CaptureAuthorizationError::WrongRoom);
    }
    if request.membership_epoch != context.membership_epoch {
        return Err(CaptureAuthorizationError::WrongMembershipEpoch);
    }
    if request.player_id != context.requester_certificate.player_id
        || request.player_id != context.provider_certificate.player_id
    {
        return Err(CaptureAuthorizationError::WrongPlayer);
    }
    if request.requester_device_id != context.requester_certificate.device_id
        || request.provider_device_id != context.provider_certificate.device_id
    {
        return Err(CaptureAuthorizationError::WrongDevice);
    }
    if context.requester_revoked || context.provider_revoked {
        return Err(CaptureAuthorizationError::RevokedDevice);
    }
    if !context
        .requester_certificate
        .is_valid_at(context.membership_epoch)
        || !context
            .provider_certificate
            .is_valid_at(context.membership_epoch)
    {
        return Err(CaptureAuthorizationError::ExpiredCertificate);
    }
    if !context
        .requester_certificate
        .has_capability(DeviceCapabilityWire::RequestCapture)
        || !context
            .provider_certificate
            .has_capability(DeviceCapabilityWire::ProvideCapture)
    {
        return Err(CaptureAuthorizationError::MissingCapability);
    }
    if request.expires_at_unix_ms < context.now_unix_ms {
        return Err(CaptureAuthorizationError::ExpiredRequest);
    }
    if request.observed_revision != context.current_revision {
        return Err(CaptureAuthorizationError::StaleRevision);
    }
    Ok(())
}

/// Ensure a request stays within one currently authorized provider offer.
pub fn authorize_capture_request_with_provider(
    advertisement: &CaptureProviderAdvertisementWire,
    request: &CaptureRequestWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    authorize_capture_request(request, context)?;
    let provider_context = CaptureProviderAuthorizationContext {
        room_id: context.room_id,
        membership_epoch: context.membership_epoch,
        now_unix_ms: context.now_unix_ms,
        provider_certificate: context.provider_certificate,
        provider_revoked: context.provider_revoked,
    };
    authorize_capture_provider(advertisement, &provider_context)?;
    if advertisement.player_id != request.player_id
        || advertisement.provider_device_id != request.provider_device_id
        || advertisement.provider_kind != request.provider_kind
        || request.max_total_bytes > advertisement.max_total_bytes
        || request.expires_at_unix_ms > advertisement.expires_at_unix_ms
        || advertisement
            .privacy_scopes
            .binary_search(&request.privacy)
            .is_err()
        || request.representations.iter().any(|representation| {
            advertisement
                .representations
                .binary_search(representation)
                .is_err()
        })
    {
        return Err(CaptureAuthorizationError::AdvertisementMismatch);
    }
    Ok(())
}

/// Bind a provider consent decision to one authorized request.
pub fn authorize_capture_consent(
    request: &CaptureRequestWire,
    consent: &CaptureConsentWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    authorize_capture_request(request, context)?;
    consent
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    authorize_provider_binding(
        request,
        &consent.request_id,
        consent.request_hash,
        &consent.room_id,
        consent.membership_epoch,
        &consent.player_id,
        &consent.requester_device_id,
        &consent.provider_device_id,
        CaptureAuthorizationError::ResponseMismatch,
    )?;
    if consent.decided_at_unix_ms > request.expires_at_unix_ms
        || consent.decided_at_unix_ms > context.now_unix_ms
    {
        return Err(CaptureAuthorizationError::ResponseMismatch);
    }
    Ok(())
}

/// Bind one monotonic provider progress update to an authorized request.
/// Sequence ordering across updates is enforced by the receiving adapter.
pub fn authorize_capture_progress(
    request: &CaptureRequestWire,
    progress: &CaptureProgressWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    authorize_capture_request(request, context)?;
    progress
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    authorize_provider_binding(
        request,
        &progress.request_id,
        progress.request_hash,
        &progress.room_id,
        progress.membership_epoch,
        &progress.player_id,
        &progress.requester_device_id,
        &progress.provider_device_id,
        CaptureAuthorizationError::ProgressMismatch,
    )?;
    if progress.completed_bytes > request.max_total_bytes
        || progress
            .total_bytes
            .is_some_and(|total| total > request.max_total_bytes)
    {
        return Err(CaptureAuthorizationError::ProgressMismatch);
    }
    Ok(())
}

/// Bind a provider response to a previously authorized request and ensure its
/// metadata cannot exceed or substitute the requested representations.
pub fn authorize_capture_response(
    request: &CaptureRequestWire,
    response: &CaptureResponseWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    authorize_capture_request(request, context)?;
    response
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    let expected_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    if response.request_id != request.request_id
        || response.request_hash != expected_hash
        || response.room_id != request.room_id
        || response.membership_epoch != request.membership_epoch
        || response.player_id != request.player_id
        || response.requester_device_id != request.requester_device_id
        || response.provider_device_id != request.provider_device_id
    {
        return Err(CaptureAuthorizationError::ResponseMismatch);
    }
    if let CaptureResponseOutcomeWire::Accepted { artifacts } = &response.outcome {
        let total_bytes = artifacts.iter().try_fold(0_u64, |total, artifact| {
            total.checked_add(artifact.transfer.byte_length)
        });
        if total_bytes.is_none_or(|total| total > request.max_total_bytes)
            || artifacts.iter().any(|artifact| {
                request
                    .representations
                    .binary_search(&artifact.representation)
                    .is_err()
                    || artifact.provider_kind != request.provider_kind
                    || artifact.captured_revision != request.observed_revision
            })
        {
            return Err(CaptureAuthorizationError::ArtifactMismatch);
        }
    }
    Ok(())
}

/// Bind a terminal receipt to the exact accepted artifact offer.
pub fn authorize_capture_completion(
    request: &CaptureRequestWire,
    response: &CaptureResponseWire,
    completion: &CaptureCompletionWire,
    context: &CaptureAuthorizationContext<'_>,
) -> Result<(), CaptureAuthorizationError> {
    authorize_capture_response(request, response, context)?;
    completion
        .validate()
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    authorize_provider_binding(
        request,
        &completion.request_id,
        completion.request_hash,
        &completion.room_id,
        completion.membership_epoch,
        &completion.player_id,
        &completion.requester_device_id,
        &completion.provider_device_id,
        CaptureAuthorizationError::CompletionMismatch,
    )?;
    let (
        CaptureResponseOutcomeWire::Accepted { artifacts: offered },
        CaptureCompletionOutcomeWire::Completed {
            artifacts: completed,
        },
    ) = (&response.outcome, &completion.outcome)
    else {
        return Err(CaptureAuthorizationError::CompletionMismatch);
    };
    if offered.len() != completed.len()
        || completed.iter().any(|receipt| {
            offered
                .iter()
                .find(|offer| offer.artifact_id == receipt.artifact_id)
                .is_none_or(|offer| {
                    offer.transfer.transfer_id != receipt.transfer_id
                        || offer.transfer.byte_length != receipt.byte_length
                        || offer.transfer.content_hash != receipt.content_hash
                })
        })
    {
        return Err(CaptureAuthorizationError::CompletionMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn authorize_provider_binding(
    request: &CaptureRequestWire,
    request_id: &CaptureRequestId,
    request_hash: SemanticHash,
    room_id: &RoomId,
    membership_epoch: u64,
    player_id: &PrincipalId,
    requester_device_id: &DeviceId,
    provider_device_id: &DeviceId,
    mismatch: CaptureAuthorizationError,
) -> Result<(), CaptureAuthorizationError> {
    let expected_hash = capture_request_hash(&request.unsigned())
        .map_err(|_| CaptureAuthorizationError::InvalidWire)?;
    if request_id != &request.request_id
        || request_hash != expected_hash
        || room_id != &request.room_id
        || membership_epoch != request.membership_epoch
        || player_id != &request.player_id
        || requester_device_id != &request.requester_device_id
        || provider_device_id != &request.provider_device_id
    {
        Err(mismatch)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CaptureArtifactDescriptorWire, CaptureArtifactId, CaptureCompletionArtifactWire,
        CaptureCompletionOutcomeWire, CaptureConsentDecisionWire, CaptureConsentPolicyWire,
        CapturePrivacyWire, CaptureProgressStageWire, CaptureProviderKindWire,
        CaptureRepresentationWire, CaptureRequestId, CaptureResponseOutcomeWire,
        CaptureTransferDescriptorWire, CaptureTransferId, CaptureViewportWire,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        DeviceCustodyWire, DeviceId, DeviceSignatureIntentWire, PrincipalId,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
        SignatureBytes, SignatureIntent, UnsignedCaptureCompletionWire, UnsignedCaptureConsentWire,
        UnsignedCaptureProgressWire, UnsignedCaptureProviderAdvertisementWire,
        UnsignedCaptureRequestWire, UnsignedCaptureResponseWire, UnsignedDeviceCertificateWire,
    };

    use super::*;

    fn key(seed: u8) -> String {
        format!("{seed:02x}").repeat(32)
    }

    fn certificate(
        player: &PrincipalId,
        seed: u8,
        capabilities: Vec<DeviceCapabilityWire>,
    ) -> DeviceCertificateWire {
        let device_key = key(seed);
        UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: poche_protocol::CertificateId::new(format!("cert-{seed}")).unwrap(),
            player_id: player.clone(),
            device_id: DeviceId::new(device_key.clone()).unwrap(),
            device_signing_public_key: device_key,
            device_encryption_public_key: "ee".repeat(32),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities,
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player.clone(),
            },
        }
        .attach_signature(SignatureBytes::new("00".repeat(64)).unwrap())
        .unwrap()
    }

    fn fixture() -> (
        CaptureRequestWire,
        DeviceCertificateWire,
        DeviceCertificateWire,
    ) {
        let player = PrincipalId::new(key(1)).unwrap();
        let requester = certificate(&player, 2, vec![DeviceCapabilityWire::RequestCapture]);
        let provider = certificate(&player, 3, vec![DeviceCapabilityWire::ProvideCapture]);
        let unsigned = UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new("capture-1").unwrap(),
            room_id: RoomId::new("room-1").unwrap(),
            membership_epoch: 1,
            player_id: player,
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            observed_revision: 12,
            expires_at_unix_ms: 20_000,
            replay_nonce: "nonce-capture-1".to_owned(),
            privacy: CapturePrivacyWire::ExactPlayerView,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            viewport: None,
            label: "playing".to_owned(),
            max_total_bytes: 1_000_000,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: requester.device_id.clone(),
            },
        };
        let request = unsigned
            .attach_signature(SignatureBytes::new("11".repeat(64)).unwrap())
            .unwrap();
        (request, requester, provider)
    }

    #[test]
    fn same_player_exact_target_is_authorized() {
        let (request, requester, provider) = fixture();
        let room = RoomId::new("room-1").unwrap();
        let context = CaptureAuthorizationContext {
            room_id: &room,
            membership_epoch: 1,
            current_revision: 12,
            now_unix_ms: 11_000,
            requester_certificate: &requester,
            provider_certificate: &provider,
            requester_revoked: false,
            provider_revoked: false,
        };
        assert_eq!(authorize_capture_request(&request, &context), Ok(()));
    }

    #[test]
    fn cross_player_stale_and_revoked_requests_fail_closed() {
        let (request, requester, provider) = fixture();
        let room = RoomId::new("room-1").unwrap();
        let other = PrincipalId::new(key(8)).unwrap();
        let other_provider = certificate(&other, 9, vec![DeviceCapabilityWire::ProvideCapture]);
        let cross_player = CaptureAuthorizationContext {
            room_id: &room,
            membership_epoch: 1,
            current_revision: 12,
            now_unix_ms: 10_000,
            requester_certificate: &requester,
            provider_certificate: &other_provider,
            requester_revoked: false,
            provider_revoked: false,
        };
        assert_eq!(
            authorize_capture_request(&request, &cross_player),
            Err(CaptureAuthorizationError::WrongPlayer)
        );

        let stale = CaptureAuthorizationContext {
            current_revision: 13,
            provider_certificate: &provider,
            ..cross_player
        };
        assert_eq!(
            authorize_capture_request(&request, &stale),
            Err(CaptureAuthorizationError::StaleRevision)
        );

        let revoked = CaptureAuthorizationContext {
            current_revision: 12,
            requester_revoked: true,
            ..stale
        };
        assert_eq!(
            authorize_capture_request(&request, &revoked),
            Err(CaptureAuthorizationError::RevokedDevice)
        );
    }

    #[test]
    fn replay_corpus_rejects_duplicate_conflict_and_overflow() {
        let (request, requester, provider) = fixture();
        let room = RoomId::new("room-1").unwrap();
        let context = CaptureAuthorizationContext {
            room_id: &room,
            membership_epoch: 1,
            current_revision: 12,
            now_unix_ms: 10_000,
            requester_certificate: &requester,
            provider_certificate: &provider,
            requester_revoked: false,
            provider_revoked: false,
        };
        let mut window = CaptureReplayWindow::new(2);
        assert_eq!(window.authorize_once(&request, &context), Ok(()));
        assert_eq!(
            window.authorize_once(&request, &context),
            Err(CaptureAuthorizationError::ReplayedRequest)
        );

        let mut conflicting = request.clone();
        conflicting.replay_nonce = "conflicting-nonce".to_owned();
        assert_eq!(
            window.authorize_once(&conflicting, &context),
            Err(CaptureAuthorizationError::ConflictingReplay)
        );

        let mut second = request.clone();
        second.request_id = CaptureRequestId::new("capture-2").unwrap();
        second.replay_nonce = "nonce-capture-2".to_owned();
        assert_eq!(window.authorize_once(&second, &context), Ok(()));
        let mut third = request.clone();
        third.request_id = CaptureRequestId::new("capture-3").unwrap();
        third.replay_nonce = "nonce-capture-3".to_owned();
        assert_eq!(
            window.authorize_once(&third, &context),
            Err(CaptureAuthorizationError::ReplayWindowFull)
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn provider_lifecycle_binds_consent_progress_offer_and_completion() {
        let (request, requester, provider) = fixture();
        let room = RoomId::new("room-1").unwrap();
        let context = CaptureAuthorizationContext {
            room_id: &room,
            membership_epoch: 1,
            current_revision: 12,
            now_unix_ms: 11_000,
            requester_certificate: &requester,
            provider_certificate: &provider,
            requester_revoked: false,
            provider_revoked: false,
        };
        let provider_intent = DeviceSignatureIntentWire {
            domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: provider.device_id.clone(),
        };
        let advertisement = UnsignedCaptureProviderAdvertisementWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: request.player_id.clone(),
            provider_device_id: provider.device_id.clone(),
            provider_kind: CaptureProviderKindWire::NativeBevy,
            representations: vec![CaptureRepresentationWire::Png],
            privacy_scopes: vec![CapturePrivacyWire::ExactPlayerView],
            consent_policy: CaptureConsentPolicyWire::UserConfirmation,
            max_total_bytes: 1_000_000,
            advertisement_sequence: 1,
            expires_at_unix_ms: 20_000,
            signature_intent: provider_intent.clone(),
        }
        .attach_signature(SignatureBytes::new("22".repeat(64)).unwrap())
        .unwrap();
        assert_eq!(
            authorize_capture_request_with_provider(&advertisement, &request, &context),
            Ok(())
        );

        let request_hash = capture_request_hash(&request.unsigned()).unwrap();
        let consent = UnsignedCaptureConsentWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: request.player_id.clone(),
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            decision: CaptureConsentDecisionWire::Granted,
            decided_at_unix_ms: 10_001,
            signature_intent: provider_intent.clone(),
        }
        .attach_signature(SignatureBytes::new("33".repeat(64)).unwrap())
        .unwrap();
        assert_eq!(
            authorize_capture_consent(&request, &consent, &context),
            Ok(())
        );

        let progress = UnsignedCaptureProgressWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: request.player_id.clone(),
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            sequence: 1,
            stage: CaptureProgressStageWire::Transferring,
            completed_bytes: 512,
            total_bytes: Some(1_024),
            signature_intent: provider_intent.clone(),
        }
        .attach_signature(SignatureBytes::new("44".repeat(64)).unwrap())
        .unwrap();
        assert_eq!(
            authorize_capture_progress(&request, &progress, &context),
            Ok(())
        );

        let descriptor = CaptureArtifactDescriptorWire {
            artifact_id: CaptureArtifactId::new("artifact-1").unwrap(),
            representation: CaptureRepresentationWire::Png,
            provider_kind: CaptureProviderKindWire::NativeBevy,
            captured_revision: 12,
            projection_hash: SemanticHash([7; 32]),
            scene_hash: Some(SemanticHash([8; 32])),
            viewport: Some(CaptureViewportWire {
                width_pixels: 640,
                height_pixels: 480,
            }),
            transfer: CaptureTransferDescriptorWire {
                transfer_id: CaptureTransferId::new("transfer-1").unwrap(),
                byte_length: 1_024,
                chunk_bytes: 1_024,
                chunk_count: 1,
                content_hash: SemanticHash([9; 32]),
            },
        };
        let response = UnsignedCaptureResponseWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: request.player_id.clone(),
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            outcome: CaptureResponseOutcomeWire::Accepted {
                artifacts: vec![descriptor.clone()],
            },
            signature_intent: provider_intent.clone(),
        }
        .attach_signature(SignatureBytes::new("55".repeat(64)).unwrap())
        .unwrap();
        assert_eq!(
            authorize_capture_response(&request, &response, &context),
            Ok(())
        );

        let completion = UnsignedCaptureCompletionWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: request.request_id.clone(),
            request_hash,
            room_id: room.clone(),
            membership_epoch: 1,
            player_id: request.player_id.clone(),
            requester_device_id: requester.device_id.clone(),
            provider_device_id: provider.device_id.clone(),
            final_sequence: 2,
            outcome: CaptureCompletionOutcomeWire::Completed {
                artifacts: vec![CaptureCompletionArtifactWire {
                    artifact_id: descriptor.artifact_id,
                    transfer_id: descriptor.transfer.transfer_id,
                    byte_length: descriptor.transfer.byte_length,
                    content_hash: descriptor.transfer.content_hash,
                }],
            },
            signature_intent: provider_intent,
        }
        .attach_signature(SignatureBytes::new("66".repeat(64)).unwrap())
        .unwrap();
        assert_eq!(
            authorize_capture_completion(&request, &response, &completion, &context),
            Ok(())
        );
        let mut substituted = completion;
        let CaptureCompletionOutcomeWire::Completed { artifacts } = &mut substituted.outcome else {
            unreachable!();
        };
        artifacts[0].content_hash = SemanticHash([10; 32]);
        assert_eq!(
            authorize_capture_completion(&request, &response, &substituted, &context),
            Err(CaptureAuthorizationError::CompletionMismatch)
        );
    }
}

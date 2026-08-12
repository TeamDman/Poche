// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pure authorization for non-authoritative cooperation between devices.

#![allow(
    clippy::missing_errors_doc,
    reason = "both public authorization functions return the documented stable redacted CaptureAuthorizationError categories"
)]

use core::fmt;

use poche_protocol::{
    CaptureRequestWire, CaptureResponseOutcomeWire, CaptureResponseWire, DeviceCapabilityWire,
    DeviceCertificateWire, RoomId, capture_request_hash,
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
        })
    }
}

impl std::error::Error for CaptureAuthorizationError {}

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
                    || artifact.captured_revision != request.observed_revision
            })
        {
            return Err(CaptureAuthorizationError::ArtifactMismatch);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CapturePrivacyWire, CaptureRepresentationWire, CaptureRequestId,
        DEVICE_COOPERATION_SCHEMA_VERSION_V1, DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
        DeviceCustodyWire, DeviceId, DeviceSignatureIntentWire, PrincipalId,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
        SignatureBytes, SignatureIntent, UnsignedCaptureRequestWire, UnsignedDeviceCertificateWire,
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
            privacy: CapturePrivacyWire::ExactPlayerView,
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
            now_unix_ms: 10_000,
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
}

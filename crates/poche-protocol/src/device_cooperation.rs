// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Signed, exact-target cooperation between independently certified devices.
//!
//! These messages are deliberately outside the authoritative game history.
//! They may describe or capture one exact-recipient view, but can never mutate
//! room state. Artifact bytes travel over a bounded private transfer selected
//! by an adapter; they are not embedded in ordinary command/AppCall replies.

#![allow(
    clippy::missing_errors_doc,
    reason = "every public validator and canonical codec returns the documented stable redacted device-cooperation error categories"
)]

use core::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::{
    CaptureArtifactId, CaptureRequestId, CaptureTransferId, DeviceId, DeviceSignatureIntentWire,
    DeviceSignatureMetadataWire, PrincipalId, RoomId, SemanticHash, SignatureAlgorithm,
    SignatureBytes,
};

/// Initial device-cooperation wire version.
pub const DEVICE_COOPERATION_SCHEMA_VERSION_V1: u16 = 1;
/// Independent signature-domain version for device-cooperation messages.
pub const DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1: u16 = 1;
/// Defensive upper bound for one complete capture artifact.
pub const MAX_CAPTURE_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
/// Payload size chosen to remain below the ordinary 30,000-byte frame limit
/// even after encrypted-transfer framing. Adapters may select a smaller value.
pub const MAX_CAPTURE_TRANSFER_CHUNK_BYTES: u32 = 24 * 1024;
/// Number of related artifacts allowed in one response bundle.
pub const MAX_CAPTURE_ARTIFACTS: usize = 4;

const CAPTURE_REQUEST_DOMAIN: &[u8] = b"POCHE\0CAPTURE-REQUEST\0V1";
const CAPTURE_RESPONSE_DOMAIN: &[u8] = b"POCHE\0CAPTURE-RESPONSE\0V1";
const CAPTURE_CANCEL_DOMAIN: &[u8] = b"POCHE\0CAPTURE-CANCEL\0V1";

/// Renderer-neutral evidence requested from one exact device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CaptureRepresentationWire {
    Png,
    SemanticHtml,
    AccessibilityTreeJson,
    LayoutJson,
}

/// Disclosure boundary of a requested capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CapturePrivacyWire {
    /// Only the public room projection may appear.
    PublicRoom,
    /// The target may render the exact private view of the shared player root.
    ExactPlayerView,
}

/// Presentation adapter that produced an artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CaptureProviderKindWire {
    NativeBevy,
    BrowserHarness,
    BrowserInteractive,
    HeadlessSemantic,
}

/// Provider-side reason that no capture artifact was returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CaptureDenialReasonWire {
    ProviderUnavailable,
    ConsentRequired,
    ConsentDenied,
    StaleRevision,
    CapabilityDenied,
    Expired,
    Cancelled,
    Busy,
    Oversize,
}

/// Requested viewport bounds. `None` lets the provider use its visible
/// surface without inventing a hidden canonical camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureViewportWire {
    pub width_pixels: u32,
    pub height_pixels: u32,
}

impl CaptureViewportWire {
    fn validate(self) -> Result<(), DeviceCooperationWireError> {
        if self.width_pixels == 0
            || self.height_pixels == 0
            || self.width_pixels > 16_384
            || self.height_pixels > 16_384
        {
            Err(DeviceCooperationWireError::InvalidRequest)
        } else {
            Ok(())
        }
    }
}

/// Device-signable capture request before its signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedCaptureRequestWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub observed_revision: u64,
    pub expires_at_unix_ms: u64,
    pub privacy: CapturePrivacyWire,
    pub representations: Vec<CaptureRepresentationWire>,
    pub viewport: Option<CaptureViewportWire>,
    pub label: String,
    pub max_total_bytes: u64,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedCaptureRequestWire {
    /// Validate the stable request shape without performing certificate or
    /// cryptographic authorization.
    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        if self.schema_version != DEVICE_COOPERATION_SCHEMA_VERSION_V1 {
            return Err(DeviceCooperationWireError::UnknownVersion);
        }
        if !self.request_id.validate()
            || !self.room_id.validate()
            || !self.player_id.validate()
            || !self.requester_device_id.validate()
            || !self.provider_device_id.validate()
            || self.membership_epoch == 0
            || self.expires_at_unix_ms == 0
            || self.label.is_empty()
            || self.label.len() > 80
            || self.label.chars().any(char::is_control)
            || self.representations.is_empty()
            || self.representations.len() > MAX_CAPTURE_ARTIFACTS
            || !strict_sorted_unique(&self.representations)
            || self.max_total_bytes == 0
            || self.max_total_bytes > MAX_CAPTURE_ARTIFACT_BYTES
        {
            return Err(DeviceCooperationWireError::InvalidRequest);
        }
        if let Some(viewport) = self.viewport {
            viewport.validate()?;
        }
        validate_device_intent(&self.signature_intent, &self.requester_device_id)
    }

    /// Attach a structurally valid device signature.
    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<CaptureRequestWire, DeviceCooperationWireError> {
        self.validate()?;
        signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)?;
        Ok(CaptureRequestWire {
            schema_version: self.schema_version,
            request_id: self.request_id,
            room_id: self.room_id,
            membership_epoch: self.membership_epoch,
            player_id: self.player_id,
            requester_device_id: self.requester_device_id,
            provider_device_id: self.provider_device_id,
            observed_revision: self.observed_revision,
            expires_at_unix_ms: self.expires_at_unix_ms,
            privacy: self.privacy,
            representations: self.representations,
            viewport: self.viewport,
            label: self.label,
            max_total_bytes: self.max_total_bytes,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Signed exact-target request. A valid signature does not by itself grant
/// capture access; both device certificates and provider policy are checked.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRequestWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub observed_revision: u64,
    pub expires_at_unix_ms: u64,
    pub privacy: CapturePrivacyWire,
    pub representations: Vec<CaptureRepresentationWire>,
    pub viewport: Option<CaptureViewportWire>,
    pub label: String,
    pub max_total_bytes: u64,
    pub signature: DeviceSignatureMetadataWire,
}

impl CaptureRequestWire {
    #[must_use]
    pub fn unsigned(&self) -> UnsignedCaptureRequestWire {
        UnsignedCaptureRequestWire {
            schema_version: self.schema_version,
            request_id: self.request_id.clone(),
            room_id: self.room_id.clone(),
            membership_epoch: self.membership_epoch,
            player_id: self.player_id.clone(),
            requester_device_id: self.requester_device_id.clone(),
            provider_device_id: self.provider_device_id.clone(),
            observed_revision: self.observed_revision,
            expires_at_unix_ms: self.expires_at_unix_ms,
            privacy: self.privacy,
            representations: self.representations.clone(),
            viewport: self.viewport,
            label: self.label.clone(),
            max_total_bytes: self.max_total_bytes,
            signature_intent: self.signature.intent(),
        }
    }

    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        self.unsigned().validate()?;
        self.signature
            .signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)
    }
}

/// Descriptor for an image-sized private transfer. The content bytes are
/// deliberately absent and must hash to `content_hash` after reassembly.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureTransferDescriptorWire {
    pub transfer_id: CaptureTransferId,
    pub byte_length: u64,
    pub chunk_bytes: u32,
    pub chunk_count: u32,
    pub content_hash: SemanticHash,
}

impl CaptureTransferDescriptorWire {
    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        if !self.transfer_id.validate()
            || self.byte_length == 0
            || self.byte_length > MAX_CAPTURE_ARTIFACT_BYTES
            || self.chunk_bytes == 0
            || self.chunk_bytes > MAX_CAPTURE_TRANSFER_CHUNK_BYTES
        {
            return Err(DeviceCooperationWireError::InvalidTransfer);
        }
        let expected = self.byte_length.div_ceil(u64::from(self.chunk_bytes));
        if u64::from(self.chunk_count) != expected {
            return Err(DeviceCooperationWireError::InvalidTransfer);
        }
        Ok(())
    }
}

/// Hash-bound metadata for one provider artifact.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureArtifactDescriptorWire {
    pub artifact_id: CaptureArtifactId,
    pub representation: CaptureRepresentationWire,
    pub provider_kind: CaptureProviderKindWire,
    pub captured_revision: u64,
    pub projection_hash: SemanticHash,
    pub scene_hash: Option<SemanticHash>,
    pub viewport: Option<CaptureViewportWire>,
    pub transfer: CaptureTransferDescriptorWire,
}

impl CaptureArtifactDescriptorWire {
    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        if !self.artifact_id.validate() {
            return Err(DeviceCooperationWireError::InvalidArtifact);
        }
        if let Some(viewport) = self.viewport {
            viewport
                .validate()
                .map_err(|_| DeviceCooperationWireError::InvalidArtifact)?;
        }
        if self.representation == CaptureRepresentationWire::Png && self.viewport.is_none() {
            return Err(DeviceCooperationWireError::InvalidArtifact);
        }
        self.transfer
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidArtifact)
    }
}

/// Provider result for one request. Accepted bundles contain metadata only;
/// transfer adapters carry the corresponding private content separately.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CaptureResponseOutcomeWire {
    Accepted {
        artifacts: Vec<CaptureArtifactDescriptorWire>,
    },
    Denied {
        reason: CaptureDenialReasonWire,
    },
}

/// Provider-signable response before its device signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedCaptureResponseWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub request_hash: SemanticHash,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub outcome: CaptureResponseOutcomeWire,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedCaptureResponseWire {
    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        if self.schema_version != DEVICE_COOPERATION_SCHEMA_VERSION_V1 {
            return Err(DeviceCooperationWireError::UnknownVersion);
        }
        if !self.request_id.validate()
            || !self.room_id.validate()
            || !self.player_id.validate()
            || !self.requester_device_id.validate()
            || !self.provider_device_id.validate()
            || self.membership_epoch == 0
        {
            return Err(DeviceCooperationWireError::InvalidResponse);
        }
        if let CaptureResponseOutcomeWire::Accepted { artifacts } = &self.outcome {
            if artifacts.is_empty()
                || artifacts.len() > MAX_CAPTURE_ARTIFACTS
                || !artifacts
                    .windows(2)
                    .all(|pair| pair[0].representation < pair[1].representation)
            {
                return Err(DeviceCooperationWireError::InvalidResponse);
            }
            for artifact in artifacts {
                artifact.validate()?;
            }
        }
        validate_device_intent(&self.signature_intent, &self.provider_device_id)
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<CaptureResponseWire, DeviceCooperationWireError> {
        self.validate()?;
        signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)?;
        Ok(CaptureResponseWire {
            schema_version: self.schema_version,
            request_id: self.request_id,
            request_hash: self.request_hash,
            room_id: self.room_id,
            membership_epoch: self.membership_epoch,
            player_id: self.player_id,
            requester_device_id: self.requester_device_id,
            provider_device_id: self.provider_device_id,
            outcome: self.outcome,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Signed provider response.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureResponseWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub request_hash: SemanticHash,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub outcome: CaptureResponseOutcomeWire,
    pub signature: DeviceSignatureMetadataWire,
}

impl CaptureResponseWire {
    #[must_use]
    pub fn unsigned(&self) -> UnsignedCaptureResponseWire {
        UnsignedCaptureResponseWire {
            schema_version: self.schema_version,
            request_id: self.request_id.clone(),
            request_hash: self.request_hash,
            room_id: self.room_id.clone(),
            membership_epoch: self.membership_epoch,
            player_id: self.player_id.clone(),
            requester_device_id: self.requester_device_id.clone(),
            provider_device_id: self.provider_device_id.clone(),
            outcome: self.outcome.clone(),
            signature_intent: self.signature.intent(),
        }
    }

    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        self.unsigned().validate()?;
        self.signature
            .signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)
    }
}

/// Signed requester cancellation. Cancellation is advisory cooperation state,
/// not a rollback or game-history mutation.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedCaptureCancelWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub request_hash: SemanticHash,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedCaptureCancelWire {
    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        if self.schema_version != DEVICE_COOPERATION_SCHEMA_VERSION_V1 {
            return Err(DeviceCooperationWireError::UnknownVersion);
        }
        if !self.request_id.validate()
            || !self.room_id.validate()
            || !self.player_id.validate()
            || !self.requester_device_id.validate()
            || !self.provider_device_id.validate()
            || self.membership_epoch == 0
        {
            return Err(DeviceCooperationWireError::InvalidCancel);
        }
        validate_device_intent(&self.signature_intent, &self.requester_device_id)
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<CaptureCancelWire, DeviceCooperationWireError> {
        self.validate()?;
        signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)?;
        Ok(CaptureCancelWire {
            schema_version: self.schema_version,
            request_id: self.request_id,
            request_hash: self.request_hash,
            room_id: self.room_id,
            membership_epoch: self.membership_epoch,
            player_id: self.player_id,
            requester_device_id: self.requester_device_id,
            provider_device_id: self.provider_device_id,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Signed requester cancellation.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureCancelWire {
    pub schema_version: u16,
    pub request_id: CaptureRequestId,
    pub request_hash: SemanticHash,
    pub room_id: RoomId,
    pub membership_epoch: u64,
    pub player_id: PrincipalId,
    pub requester_device_id: DeviceId,
    pub provider_device_id: DeviceId,
    pub signature: DeviceSignatureMetadataWire,
}

impl CaptureCancelWire {
    #[must_use]
    pub fn unsigned(&self) -> UnsignedCaptureCancelWire {
        UnsignedCaptureCancelWire {
            schema_version: self.schema_version,
            request_id: self.request_id.clone(),
            request_hash: self.request_hash,
            room_id: self.room_id.clone(),
            membership_epoch: self.membership_epoch,
            player_id: self.player_id.clone(),
            requester_device_id: self.requester_device_id.clone(),
            provider_device_id: self.provider_device_id.clone(),
            signature_intent: self.signature.intent(),
        }
    }

    pub fn validate(&self) -> Result<(), DeviceCooperationWireError> {
        self.unsigned().validate()?;
        self.signature
            .signature
            .validate()
            .map_err(|_| DeviceCooperationWireError::InvalidSignature)
    }
}

/// Stable, redacted structural failure for device-cooperation messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceCooperationWireError {
    UnknownVersion,
    InvalidRequest,
    InvalidResponse,
    InvalidCancel,
    InvalidArtifact,
    InvalidTransfer,
    InvalidSignatureIntent,
    InvalidSignature,
    Encoding,
}

impl fmt::Display for DeviceCooperationWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownVersion => "unknown device-cooperation schema version",
            Self::InvalidRequest => "capture request is invalid",
            Self::InvalidResponse => "capture response is invalid",
            Self::InvalidCancel => "capture cancellation is invalid",
            Self::InvalidArtifact => "capture artifact descriptor is invalid",
            Self::InvalidTransfer => "capture transfer descriptor is invalid",
            Self::InvalidSignatureIntent => "device signature intent is invalid",
            Self::InvalidSignature => "device signature is invalid",
            Self::Encoding => "device-cooperation canonical encoding failed",
        })
    }
}

impl std::error::Error for DeviceCooperationWireError {}

/// Canonical device-signing bytes for a capture request.
pub fn canonical_capture_request_bytes(
    request: &UnsignedCaptureRequestWire,
) -> Result<Vec<u8>, DeviceCooperationWireError> {
    request.validate()?;
    encode_json_domain(CAPTURE_REQUEST_DOMAIN, request)
}

/// Stable semantic binding carried by responses and cancellations.
pub fn capture_request_hash(
    request: &UnsignedCaptureRequestWire,
) -> Result<SemanticHash, DeviceCooperationWireError> {
    Ok(SemanticHash(
        *blake3::hash(&canonical_capture_request_bytes(request)?).as_bytes(),
    ))
}

/// Canonical provider-signing bytes for a capture response.
pub fn canonical_capture_response_bytes(
    response: &UnsignedCaptureResponseWire,
) -> Result<Vec<u8>, DeviceCooperationWireError> {
    response.validate()?;
    encode_json_domain(CAPTURE_RESPONSE_DOMAIN, response)
}

/// Canonical requester-signing bytes for capture cancellation.
pub fn canonical_capture_cancel_bytes(
    cancel: &UnsignedCaptureCancelWire,
) -> Result<Vec<u8>, DeviceCooperationWireError> {
    cancel.validate()?;
    encode_json_domain(CAPTURE_CANCEL_DOMAIN, cancel)
}

fn validate_device_intent(
    intent: &DeviceSignatureIntentWire,
    expected: &DeviceId,
) -> Result<(), DeviceCooperationWireError> {
    if intent.domain_version == DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1
        && intent.algorithm == SignatureAlgorithm::Ed25519
        && &intent.key_id == expected
    {
        Ok(())
    } else {
        Err(DeviceCooperationWireError::InvalidSignatureIntent)
    }
}

fn strict_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn encode_json_domain<T: Serialize>(
    domain: &[u8],
    value: &T,
) -> Result<Vec<u8>, DeviceCooperationWireError> {
    let json = serde_json::to_vec(value).map_err(|_| DeviceCooperationWireError::Encoding)?;
    let length = u64::try_from(json.len()).map_err(|_| DeviceCooperationWireError::Encoding)?;
    let mut bytes = Vec::with_capacity(domain.len() + 8 + json.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&json);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> UnsignedCaptureRequestWire {
        let requester = DeviceId::new("device-requester").unwrap();
        UnsignedCaptureRequestWire {
            schema_version: DEVICE_COOPERATION_SCHEMA_VERSION_V1,
            request_id: CaptureRequestId::new("capture-1").unwrap(),
            room_id: RoomId::new("room-1").unwrap(),
            membership_epoch: 3,
            player_id: PrincipalId::new("player-1").unwrap(),
            requester_device_id: requester.clone(),
            provider_device_id: DeviceId::new("device-renderer").unwrap(),
            observed_revision: 42,
            expires_at_unix_ms: 2_000_000_000_000,
            privacy: CapturePrivacyWire::ExactPlayerView,
            representations: vec![
                CaptureRepresentationWire::Png,
                CaptureRepresentationWire::LayoutJson,
            ],
            viewport: Some(CaptureViewportWire {
                width_pixels: 1280,
                height_pixels: 720,
            }),
            label: "bidding".to_owned(),
            max_total_bytes: 8 * 1024 * 1024,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_COOPERATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: requester,
            },
        }
    }

    #[test]
    fn exact_target_and_revision_change_the_signed_request() {
        let request = request();
        let original = capture_request_hash(&request).unwrap();
        let mut changed = request.clone();
        changed.observed_revision += 1;
        assert_ne!(original, capture_request_hash(&changed).unwrap());
        changed = request.clone();
        changed.provider_device_id = DeviceId::new("other-renderer").unwrap();
        assert_ne!(original, capture_request_hash(&changed).unwrap());
    }

    #[test]
    fn request_rejects_duplicates_oversize_and_wrong_signer() {
        let mut invalid = request();
        invalid.representations = vec![
            CaptureRepresentationWire::Png,
            CaptureRepresentationWire::Png,
        ];
        assert_eq!(
            invalid.validate(),
            Err(DeviceCooperationWireError::InvalidRequest)
        );

        let mut invalid = request();
        invalid.max_total_bytes = MAX_CAPTURE_ARTIFACT_BYTES + 1;
        assert_eq!(
            invalid.validate(),
            Err(DeviceCooperationWireError::InvalidRequest)
        );

        let mut invalid = request();
        invalid.signature_intent.key_id = DeviceId::new("wrong-device").unwrap();
        assert_eq!(
            invalid.validate(),
            Err(DeviceCooperationWireError::InvalidSignatureIntent)
        );
    }

    #[test]
    fn transfer_descriptor_is_exact_and_bounded() {
        let valid = CaptureTransferDescriptorWire {
            transfer_id: CaptureTransferId::new("transfer-1").unwrap(),
            byte_length: 49_000,
            chunk_bytes: 24_000,
            chunk_count: 3,
            content_hash: SemanticHash([7; 32]),
        };
        assert_eq!(valid.validate(), Ok(()));

        let mut invalid = valid;
        invalid.chunk_count = 2;
        assert_eq!(
            invalid.validate(),
            Err(DeviceCooperationWireError::InvalidTransfer)
        );
    }
}

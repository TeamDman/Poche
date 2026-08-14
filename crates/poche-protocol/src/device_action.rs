// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Root-certified, device-signed ordinary player action transport.

#![allow(
    clippy::missing_errors_doc,
    reason = "every public validator and canonical codec returns stable redacted device-action categories"
)]

use core::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::{
    CommandId, CommandPayload, CorrelationId, DeviceCapabilityWire, DeviceCertificateWire,
    DeviceId, DeviceSignatureIntentWire, DeviceSignatureMetadataWire, InviteProof, PrincipalId,
    RoomId, SemanticHash, SignatureAlgorithm, SignatureBytes,
};

pub const DEVICE_ACTION_SCHEMA_VERSION_V1: u16 = 1;
pub const DEVICE_ACTION_SIGNATURE_DOMAIN_V1: u16 = 1;

const DEVICE_ACTION_DOMAIN: &[u8] = b"POCHE\0DEVICE-ACTION\0V1";
const DEVICE_OBSERVATION_REQUEST_DOMAIN: &[u8] = b"POCHE\0DEVICE-OBSERVATION-REQUEST\0V1";
const MAX_ACTION_ID_BYTES: usize = 96;

/// Exact action request before the certified device signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedDeviceActionWire {
    pub schema_version: u16,
    pub certificate: DeviceCertificateWire,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub command_id: CommandId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub expected_revision: u64,
    pub expected_projection_hash: SemanticHash,
    pub action_id: String,
    pub payload: CommandPayload,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedDeviceActionWire {
    pub fn validate(&self) -> Result<(), DeviceActionWireError> {
        self.certificate
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidCertificate)?;
        if self.schema_version != DEVICE_ACTION_SCHEMA_VERSION_V1 {
            return Err(DeviceActionWireError::UnknownVersion);
        }
        let bootstrap = self.session_epoch == 0
            && self.expected_revision == 0
            && matches!(self.payload, CommandPayload::CreateRoom)
            && self.certificate.valid_from_membership_epoch == 1;
        if !self.room_id.validate()
            || !self.command_id.validate()
            || self.player_id != self.certificate.player_id
            || self.device_id != self.certificate.device_id
            || (!bootstrap && !self.certificate.is_valid_at(self.session_epoch))
            || !self
                .certificate
                .has_capability(DeviceCapabilityWire::Propose)
            || !valid_action_id(&self.action_id)
        {
            return Err(DeviceActionWireError::InvalidBinding);
        }
        if self.signature_intent.domain_version != DEVICE_ACTION_SIGNATURE_DOMAIN_V1
            || self.signature_intent.algorithm != SignatureAlgorithm::Ed25519
            || self.signature_intent.key_id != self.device_id
        {
            return Err(DeviceActionWireError::InvalidSignatureIntent);
        }
        Ok(())
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<DeviceActionWire, DeviceActionWireError> {
        self.validate()?;
        signature
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidSignature)?;
        Ok(DeviceActionWire {
            schema_version: self.schema_version,
            certificate: self.certificate,
            room_id: self.room_id,
            session_epoch: self.session_epoch,
            command_id: self.command_id,
            player_id: self.player_id,
            device_id: self.device_id,
            expected_revision: self.expected_revision,
            expected_projection_hash: self.expected_projection_hash,
            action_id: self.action_id,
            payload: self.payload,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Root-certified action proposal signed by the exact proposing device.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceActionWire {
    pub schema_version: u16,
    pub certificate: DeviceCertificateWire,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub command_id: CommandId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub expected_revision: u64,
    pub expected_projection_hash: SemanticHash,
    pub action_id: String,
    pub payload: CommandPayload,
    pub signature: DeviceSignatureMetadataWire,
}

impl DeviceActionWire {
    pub fn validate(&self) -> Result<(), DeviceActionWireError> {
        self.unsigned().validate()?;
        self.signature
            .signature
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidSignature)
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedDeviceActionWire {
        UnsignedDeviceActionWire {
            schema_version: self.schema_version,
            certificate: self.certificate.clone(),
            room_id: self.room_id.clone(),
            session_epoch: self.session_epoch,
            command_id: self.command_id.clone(),
            player_id: self.player_id.clone(),
            device_id: self.device_id.clone(),
            expected_revision: self.expected_revision,
            expected_projection_hash: self.expected_projection_hash,
            action_id: self.action_id.clone(),
            payload: self.payload.clone(),
            signature_intent: self.signature.intent(),
        }
    }
}

/// Stable public validation categories that never retain rejected wire bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceActionWireError {
    UnknownVersion,
    InvalidCertificate,
    InvalidBinding,
    InvalidSignatureIntent,
    InvalidSignature,
    Encoding,
}

impl fmt::Display for DeviceActionWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownVersion => "unknown device-action schema version",
            Self::InvalidCertificate => "device-action certificate is invalid",
            Self::InvalidBinding => "device-action identity or revision binding is invalid",
            Self::InvalidSignatureIntent => "device-action signature intent is invalid",
            Self::InvalidSignature => "device-action signature is invalid",
            Self::Encoding => "device-action canonical encoding failed",
        })
    }
}

impl std::error::Error for DeviceActionWireError {}

/// Canonical device-signing bytes for an ordinary action proposal.
pub fn canonical_device_action_bytes(
    action: &UnsignedDeviceActionWire,
) -> Result<Vec<u8>, DeviceActionWireError> {
    action.validate()?;
    let json = serde_json::to_vec(action).map_err(|_| DeviceActionWireError::Encoding)?;
    let length = u64::try_from(json.len()).map_err(|_| DeviceActionWireError::Encoding)?;
    let mut bytes = Vec::with_capacity(DEVICE_ACTION_DOMAIN.len() + 8 + json.len());
    bytes.extend_from_slice(DEVICE_ACTION_DOMAIN);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&json);
    Ok(bytes)
}

fn valid_action_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ACTION_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Whether the caller wants an immediate snapshot or a strictly later view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceObservationModeWire {
    Snapshot,
    Wait { after_revision: u64 },
}

/// Device-authenticated exact-recipient read before its signature is attached.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedDeviceObservationRequestWire {
    pub schema_version: u16,
    pub certificate: DeviceCertificateWire,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub request_id: CorrelationId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub mode: DeviceObservationModeWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_invite: Option<InviteProof>,
    pub signature_intent: DeviceSignatureIntentWire,
}

impl UnsignedDeviceObservationRequestWire {
    pub fn validate(&self) -> Result<(), DeviceActionWireError> {
        self.certificate
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidCertificate)?;
        if self.schema_version != DEVICE_ACTION_SCHEMA_VERSION_V1 {
            return Err(DeviceActionWireError::UnknownVersion);
        }
        let bootstrap = self.session_epoch == 0
            && matches!(self.mode, DeviceObservationModeWire::Snapshot)
            && self.join_invite.is_none()
            && self.certificate.valid_from_membership_epoch == 1;
        let valid_join_discovery = self.join_invite.as_ref().is_none_or(|invite| {
            self.session_epoch > 0
                && matches!(self.mode, DeviceObservationModeWire::Snapshot)
                && !invite.expose().is_empty()
                && invite.expose().len() <= 256
        });
        if !self.room_id.validate()
            || !self.request_id.validate()
            || self.player_id != self.certificate.player_id
            || self.device_id != self.certificate.device_id
            || (!bootstrap && !self.certificate.is_valid_at(self.session_epoch))
            || !valid_join_discovery
            || !self
                .certificate
                .has_capability(DeviceCapabilityWire::ReceivePrivateProjection)
        {
            return Err(DeviceActionWireError::InvalidBinding);
        }
        if self.signature_intent.domain_version != DEVICE_ACTION_SIGNATURE_DOMAIN_V1
            || self.signature_intent.algorithm != SignatureAlgorithm::Ed25519
            || self.signature_intent.key_id != self.device_id
        {
            return Err(DeviceActionWireError::InvalidSignatureIntent);
        }
        Ok(())
    }

    pub fn attach_signature(
        self,
        signature: SignatureBytes,
    ) -> Result<DeviceObservationRequestWire, DeviceActionWireError> {
        self.validate()?;
        signature
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidSignature)?;
        Ok(DeviceObservationRequestWire {
            schema_version: self.schema_version,
            certificate: self.certificate,
            room_id: self.room_id,
            session_epoch: self.session_epoch,
            request_id: self.request_id,
            player_id: self.player_id,
            device_id: self.device_id,
            mode: self.mode,
            join_invite: self.join_invite,
            signature: DeviceSignatureMetadataWire {
                domain_version: self.signature_intent.domain_version,
                algorithm: self.signature_intent.algorithm,
                key_id: self.signature_intent.key_id,
                signature,
            },
        })
    }
}

/// Root-certified, device-signed exact-recipient read or wait request.
#[derive(Clone, Debug, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceObservationRequestWire {
    pub schema_version: u16,
    pub certificate: DeviceCertificateWire,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub request_id: CorrelationId,
    pub player_id: PrincipalId,
    pub device_id: DeviceId,
    pub mode: DeviceObservationModeWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_invite: Option<InviteProof>,
    pub signature: DeviceSignatureMetadataWire,
}

impl DeviceObservationRequestWire {
    pub fn validate(&self) -> Result<(), DeviceActionWireError> {
        self.unsigned().validate()?;
        self.signature
            .signature
            .validate()
            .map_err(|_| DeviceActionWireError::InvalidSignature)
    }

    #[must_use]
    pub fn unsigned(&self) -> UnsignedDeviceObservationRequestWire {
        UnsignedDeviceObservationRequestWire {
            schema_version: self.schema_version,
            certificate: self.certificate.clone(),
            room_id: self.room_id.clone(),
            session_epoch: self.session_epoch,
            request_id: self.request_id.clone(),
            player_id: self.player_id.clone(),
            device_id: self.device_id.clone(),
            mode: self.mode,
            join_invite: self.join_invite.clone(),
            signature_intent: self.signature.intent(),
        }
    }
}

/// Canonical device-signing bytes for a private observation/wait request.
pub fn canonical_device_observation_request_bytes(
    request: &UnsignedDeviceObservationRequestWire,
) -> Result<Vec<u8>, DeviceActionWireError> {
    request.validate()?;
    let json = serde_json::to_vec(request).map_err(|_| DeviceActionWireError::Encoding)?;
    let length = u64::try_from(json.len()).map_err(|_| DeviceActionWireError::Encoding)?;
    let mut bytes = Vec::with_capacity(DEVICE_OBSERVATION_REQUEST_DOMAIN.len() + 8 + json.len());
    bytes.extend_from_slice(DEVICE_OBSERVATION_REQUEST_DOMAIN);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&json);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use crate::{
        CertificateId, DeviceCustodyWire, REPLICATION_SCHEMA_VERSION_V1,
        REPLICATION_SIGNATURE_DOMAIN_V1, SignatureIntent, UnsignedDeviceCertificateWire,
    };

    use super::*;

    fn action() -> UnsignedDeviceActionWire {
        let player_id = PrincipalId::new("11".repeat(32)).unwrap();
        let device_id = DeviceId::new("22".repeat(32)).unwrap();
        let certificate = UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new("device-action-test").unwrap(),
            player_id: player_id.clone(),
            device_id: device_id.clone(),
            device_signing_public_key: device_id.as_str().to_owned(),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![DeviceCapabilityWire::Propose],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: player_id.clone(),
            },
        }
        .attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
        .unwrap();
        UnsignedDeviceActionWire {
            schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
            certificate,
            room_id: RoomId::new("room-1").unwrap(),
            session_epoch: 1,
            command_id: CommandId::new("command-1").unwrap(),
            player_id,
            device_id: device_id.clone(),
            expected_revision: 7,
            expected_projection_hash: SemanticHash([7; 32]),
            action_id: "game-bid-1".to_owned(),
            payload: CommandPayload::GameAction {
                action: crate::GameActionWire::Bid { tricks: 1 },
            },
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: device_id,
            },
        }
    }

    #[test]
    fn signature_bytes_bind_revision_hash_action_and_payload() {
        let original = action();
        let bytes = canonical_device_action_bytes(&original).unwrap();
        for changed in [
            {
                let mut changed = original.clone();
                changed.expected_revision += 1;
                changed
            },
            {
                let mut changed = original.clone();
                changed.expected_projection_hash = SemanticHash([8; 32]);
                changed
            },
            {
                let mut changed = original.clone();
                changed.action_id = "game-bid-2".to_owned();
                changed
            },
            {
                let mut changed = original.clone();
                changed.payload = CommandPayload::GameAction {
                    action: crate::GameActionWire::Bid { tricks: 2 },
                };
                changed
            },
        ] {
            assert_ne!(canonical_device_action_bytes(&changed).unwrap(), bytes);
        }
    }

    #[test]
    fn action_requires_propose_capability_and_exact_device_binding() {
        let mut missing = action();
        missing.certificate.capabilities = vec![DeviceCapabilityWire::Vote];
        assert_eq!(
            missing.validate(),
            Err(DeviceActionWireError::InvalidBinding)
        );

        let mut wrong_device = action();
        wrong_device.device_id = DeviceId::new("33".repeat(32)).unwrap();
        assert_eq!(
            wrong_device.validate(),
            Err(DeviceActionWireError::InvalidBinding)
        );
    }

    #[test]
    fn observation_request_requires_private_projection_capability() {
        let action = action();
        let request = UnsignedDeviceObservationRequestWire {
            schema_version: DEVICE_ACTION_SCHEMA_VERSION_V1,
            certificate: action.certificate.clone(),
            room_id: action.room_id,
            session_epoch: action.session_epoch,
            request_id: CorrelationId::new("observe-1").unwrap(),
            player_id: action.player_id,
            device_id: action.device_id.clone(),
            mode: DeviceObservationModeWire::Snapshot,
            join_invite: None,
            signature_intent: DeviceSignatureIntentWire {
                domain_version: DEVICE_ACTION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: action.device_id,
            },
        };
        assert_eq!(
            request.validate(),
            Err(DeviceActionWireError::InvalidBinding)
        );
        let mut allowed = request;
        allowed.certificate.capabilities = vec![
            DeviceCapabilityWire::Propose,
            DeviceCapabilityWire::ReceivePrivateProjection,
        ];
        assert!(allowed.validate().is_ok());
        let snapshot = canonical_device_observation_request_bytes(&allowed).unwrap();
        allowed.join_invite = Some(InviteProof::new("join-secret").unwrap());
        let joined = canonical_device_observation_request_bytes(&allowed).unwrap();
        assert_ne!(joined, snapshot);
        allowed.mode = DeviceObservationModeWire::Wait { after_revision: 7 };
        assert_eq!(
            allowed.validate(),
            Err(DeviceActionWireError::InvalidBinding),
            "invite material is valid only on a signed immediate discovery request"
        );
        allowed.join_invite = None;
        assert_ne!(
            canonical_device_observation_request_bytes(&allowed).unwrap(),
            snapshot
        );
    }
}

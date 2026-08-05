// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use core::fmt;

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{PrincipalId, RoomId, SignatureBytes};
use serde::{Deserialize, Serialize};

pub(crate) const ENCRYPTED_PROJECTION_SCHEMA_VERSION: u16 = 1;
pub(crate) const MAX_ENCRYPTED_PROJECTION_BYTES: usize = 30_000;

/// The exact released Veilid construction used for recipient-private output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionEncryptionAlgorithm {
    Vld0HpkeBase,
}

/// Capturable network packet. Only routing metadata and an opaque HPKE blob
/// are visible before the exact stable recipient opens it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedProjectionPacket {
    pub schema_version: u16,
    pub algorithm: ProjectionEncryptionAlgorithm,
    pub room_id: RoomId,
    pub session_epoch: u64,
    pub host_principal_id: PrincipalId,
    pub recipient_principal_id: PrincipalId,
    pub projection_epoch: u64,
    pub current_revision: u64,
    pub(crate) sealed_base64url: String,
    pub(crate) host_signature: SignatureBytes,
}

impl fmt::Debug for EncryptedProjectionPacket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncryptedProjectionPacket")
            .field("schema_version", &self.schema_version)
            .field("algorithm", &self.algorithm)
            .field("room_id", &self.room_id)
            .field("session_epoch", &self.session_epoch)
            .field("host_principal_id", &self.host_principal_id)
            .field("recipient_principal_id", &self.recipient_principal_id)
            .field("projection_epoch", &self.projection_epoch)
            .field("current_revision", &self.current_revision)
            .field("sealed_base64url", &"<opaque-hpke-payload>")
            .field("host_signature", &"<signature>")
            .finish()
    }
}

/// Stable fail-closed projection privacy categories. Rejected bytes and
/// backend diagnostics are deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionCryptoError {
    InvalidIdentity,
    InvalidProjection,
    WrongRecipient,
    StaleProjection,
    InvalidPacket,
    NonCanonical,
    Oversized,
    BadHostSignature,
    EncryptFailed,
    DecryptFailed,
    Unavailable,
}

impl EncryptedProjectionPacket {
    /// Encode canonical strict JSON below Poche's safe `AppCall` ceiling.
    ///
    /// # Errors
    ///
    /// Rejects invalid metadata, malformed ciphertext, serialization failure,
    /// or output above the safe transport ceiling.
    pub fn encode(&self) -> Result<Vec<u8>, ProjectionCryptoError> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|_| ProjectionCryptoError::InvalidPacket)?;
        if encoded.len() > MAX_ENCRYPTED_PROJECTION_BYTES {
            return Err(ProjectionCryptoError::Oversized);
        }
        Ok(encoded)
    }

    /// Decode and re-encode a strict canonical captured packet.
    ///
    /// # Errors
    ///
    /// Rejects unknown fields, malformed identifiers/ciphertext, noncanonical
    /// JSON, and size violations without retaining input.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProjectionCryptoError> {
        if bytes.len() > MAX_ENCRYPTED_PROJECTION_BYTES {
            return Err(ProjectionCryptoError::Oversized);
        }
        let packet: Self =
            serde_json::from_slice(bytes).map_err(|_| ProjectionCryptoError::InvalidPacket)?;
        packet.validate()?;
        let canonical =
            serde_json::to_vec(&packet).map_err(|_| ProjectionCryptoError::InvalidPacket)?;
        if canonical != bytes {
            return Err(ProjectionCryptoError::NonCanonical);
        }
        Ok(packet)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectionCryptoError> {
        if self.schema_version != ENCRYPTED_PROJECTION_SCHEMA_VERSION
            || self.algorithm != ProjectionEncryptionAlgorithm::Vld0HpkeBase
            || RoomId::new(self.room_id.as_str()).is_err()
            || PrincipalId::new(self.host_principal_id.as_str()).is_err()
            || PrincipalId::new(self.recipient_principal_id.as_str()).is_err()
            || self.session_epoch == 0
            || self.sealed_base64url.is_empty()
            || SignatureBytes::new(self.host_signature.as_str()).is_err()
        {
            return Err(ProjectionCryptoError::InvalidPacket);
        }
        let sealed = self.sealed_bytes()?;
        if sealed.len() > MAX_ENCRYPTED_PROJECTION_BYTES {
            return Err(ProjectionCryptoError::Oversized);
        }
        if BASE64URL_NOPAD.encode(&sealed) != self.sealed_base64url {
            return Err(ProjectionCryptoError::InvalidPacket);
        }
        Ok(())
    }

    pub(crate) fn sealed_bytes(&self) -> Result<Vec<u8>, ProjectionCryptoError> {
        BASE64URL_NOPAD
            .decode(self.sealed_base64url.as_bytes())
            .map_err(|_| ProjectionCryptoError::InvalidPacket)
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Transport-only HTTP/Veilid relay shapes for private capture artifacts.

use core::fmt;

use poche_protocol::{
    CaptureProviderAdvertisementWire, CaptureRequestId, CaptureRequestWire, CaptureResponseWire,
    CaptureTransferId, DeviceCertificateWire, DeviceId,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{EncryptedCaptureChunk, WrappedCaptureTransferKey};

/// Opaque bearer capability issued only after a signed provider registration
/// or signed requester capture call. It is transport authority for one relay
/// lane, never player/game authority, and its debug projection is redacted.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptureRelayToken(String);

impl CaptureRelayToken {
    pub fn random() -> Result<Self, CaptureRelayWireError> {
        let mut bytes = Zeroizing::new([0_u8; 32]);
        getrandom::fill(&mut *bytes).map_err(|_| CaptureRelayWireError::RandomUnavailable)?;
        Ok(Self(hex(&*bytes)))
    }

    pub fn validate(&self) -> Result<(), CaptureRelayWireError> {
        if self.0.len() == 64
            && self
                .0
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            Ok(())
        } else {
            Err(CaptureRelayWireError::InvalidToken)
        }
    }

    #[must_use]
    pub fn expose_to_transport(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CaptureRelayToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CaptureRelayToken(<redacted>)")
    }
}

/// Register one already-signed provider advertisement and root certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderRegistrationCall {
    pub certificate: DeviceCertificateWire,
    pub advertisement: CaptureProviderAdvertisementWire,
}

/// Private provider polling capability. The token is never exposed by list
/// or provider-discovery responses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderRegistrationReceipt {
    pub provider_device_id: DeviceId,
    pub provider_token: CaptureRelayToken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderPollCall {
    pub provider_token: CaptureRelayToken,
}

/// Explicitly retire one provider lane. In-flight artifact jobs retain their
/// independently scoped requester delivery capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderUnregisterCall {
    pub provider_token: CaptureRelayToken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderUnregisterReceipt {
    pub provider_device_id: DeviceId,
}

/// Exact request plus the certified recipient key a provider needs to wrap
/// artifact keys. No requester secret or omniscient projection is present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRelayJob {
    pub request: CaptureRequestWire,
    pub requester_certificate: DeviceCertificateWire,
}

/// Provider's signed metadata reply and one key envelope per offered artifact.
/// Raw artifact bytes remain absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureProviderResponseCall {
    pub provider_token: CaptureRelayToken,
    pub response: CaptureResponseWire,
    pub wrapped_keys: Vec<WrappedCaptureTransferKey>,
}

/// One independently bounded encrypted chunk upload on the artifact lane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureChunkUploadCall {
    pub provider_token: CaptureRelayToken,
    pub request_id: CaptureRequestId,
    pub chunk: EncryptedCaptureChunk,
}

/// Result of the signed cooperation metadata exchange plus a private bearer
/// capability scoped to the requester's exact artifact delivery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRequestRelayReceipt {
    pub response: CaptureResponseWire,
    pub delivery_token: CaptureRelayToken,
    pub wrapped_keys: Vec<WrappedCaptureTransferKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureChunkFetchCall {
    pub delivery_token: CaptureRelayToken,
    pub request_id: CaptureRequestId,
    pub transfer_id: CaptureTransferId,
    pub chunk_index: u32,
}

/// A chunk is either not uploaded yet or available exactly. Absence is an
/// inspectable retry state, not a fabricated empty artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum CaptureChunkFetchResult {
    Pending,
    Available(EncryptedCaptureChunk),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureDeliveryAcknowledgeCall {
    pub delivery_token: CaptureRelayToken,
    pub request_id: CaptureRequestId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRelayAcknowledgement {
    pub request_id: CaptureRequestId,
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureRelayWireError {
    InvalidToken,
    RandomUnavailable,
}

impl fmt::Display for CaptureRelayWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidToken => "capture relay capability is invalid",
            Self::RandomUnavailable => "capture relay randomness is unavailable",
        })
    }
}

impl std::error::Error for CaptureRelayWireError {}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_tokens_are_random_valid_and_redacted() {
        let first = CaptureRelayToken::random().unwrap();
        let second = CaptureRelayToken::random().unwrap();
        assert_ne!(first, second);
        assert!(first.validate().is_ok());
        assert_eq!(format!("{first:?}"), "CaptureRelayToken(<redacted>)");
        assert!(!format!("{first:?}").contains(first.expose_to_transport()));
    }
}

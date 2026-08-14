// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exact-device wrapping for short-lived artifact content keys.

use core::fmt;

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use curve25519_dalek::montgomery::MontgomeryPoint;
use poche_protocol::{CertificateId, DeviceCertificateWire, DeviceId, SemanticHash};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::CaptureTransferKey;

const KEY_WRAP_SCHEMA_VERSION_V1: u16 = 1;
const CONTENT_KEY_BYTES: usize = 32;
const AEAD_TAG_BYTES: usize = 16;

/// One artifact content key sealed to the X25519 recipient key bound into a
/// root-certified device certificate. A facilitator may store and forward
/// this object but cannot recover the plaintext key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrappedCaptureTransferKey {
    pub schema_version: u16,
    pub recipient_device_id: DeviceId,
    pub recipient_certificate_id: CertificateId,
    pub recipient_encryption_key_hash: SemanticHash,
    pub transfer_id: poche_protocol::CaptureTransferId,
    pub request_hash: SemanticHash,
    pub ephemeral_public_key: String,
    pub ciphertext: Vec<u8>,
}

impl fmt::Debug for WrappedCaptureTransferKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WrappedCaptureTransferKey")
            .field("schema_version", &self.schema_version)
            .field("recipient_device_id", &self.recipient_device_id)
            .field("recipient_certificate_id", &self.recipient_certificate_id)
            .field(
                "recipient_encryption_key_hash",
                &self.recipient_encryption_key_hash,
            )
            .field("transfer_id", &self.transfer_id)
            .field("request_hash", &self.request_hash)
            .field("ephemeral_public_key", &self.ephemeral_public_key)
            .field("ciphertext", &"<encrypted-content-key>")
            .finish()
    }
}

impl WrappedCaptureTransferKey {
    pub fn validate(&self) -> Result<(), CaptureKeyWrapError> {
        let ephemeral = decode_hex::<32>(&self.ephemeral_public_key)
            .ok_or(CaptureKeyWrapError::InvalidEnvelope)?;
        if self.schema_version != KEY_WRAP_SCHEMA_VERSION_V1
            || !self.recipient_device_id.validate()
            || !self.recipient_certificate_id.validate()
            || !self.transfer_id.validate()
            || ephemeral == [0; 32]
            || self.ciphertext.len() != CONTENT_KEY_BYTES + AEAD_TAG_BYTES
        {
            Err(CaptureKeyWrapError::InvalidEnvelope)
        } else {
            Ok(())
        }
    }
}

/// Derive a purpose-separated X25519 public key from the protected seed that
/// also roots a device's Ed25519 signing key. The signing and encryption
/// scalar material are distinct even though one protected profile handle owns
/// their common random seed.
#[must_use]
pub fn device_encryption_public_key(signing_seed: &[u8; 32]) -> [u8; 32] {
    let secret = device_encryption_secret(signing_seed);
    MontgomeryPoint::mul_base_clamped(*secret).to_bytes()
}

/// Generate one random content key, retain it only in the provider-side
/// transfer key, and seal an independent copy to the requester's certified
/// encryption key.
pub fn generate_wrapped_capture_transfer_key(
    recipient: &DeviceCertificateWire,
    transfer_id: poche_protocol::CaptureTransferId,
    request_hash: SemanticHash,
) -> Result<(CaptureTransferKey, WrappedCaptureTransferKey), CaptureKeyWrapError> {
    recipient
        .validate()
        .map_err(|_| CaptureKeyWrapError::InvalidCertificate)?;
    let recipient_public = decode_hex::<32>(&recipient.device_encryption_public_key)
        .ok_or(CaptureKeyWrapError::InvalidCertificate)?;
    if recipient_public == [0; 32] {
        return Err(CaptureKeyWrapError::InvalidCertificate);
    }
    let mut ephemeral_secret = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *ephemeral_secret).map_err(|_| CaptureKeyWrapError::RandomUnavailable)?;
    let ephemeral_public = MontgomeryPoint::mul_base_clamped(*ephemeral_secret).to_bytes();
    let shared = MontgomeryPoint(recipient_public)
        .mul_clamped(*ephemeral_secret)
        .to_bytes();
    if shared == [0; 32] {
        return Err(CaptureKeyWrapError::Crypto);
    }
    let mut content_key = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *content_key).map_err(|_| CaptureKeyWrapError::RandomUnavailable)?;
    let envelope =
        envelope_without_ciphertext(recipient, transfer_id, request_hash, ephemeral_public);
    let ciphertext = seal_content_key(&envelope, shared, &content_key)?;
    let transfer_key = CaptureTransferKey::new(*content_key);
    Ok((
        transfer_key,
        WrappedCaptureTransferKey {
            ciphertext,
            ..envelope
        },
    ))
}

/// Open a wrapped artifact key only when the protected device seed derives the
/// exact encryption public key bound into the named certificate.
pub fn open_wrapped_capture_transfer_key(
    wrapped: &WrappedCaptureTransferKey,
    recipient: &DeviceCertificateWire,
    signing_seed: &[u8; 32],
) -> Result<CaptureTransferKey, CaptureKeyWrapError> {
    wrapped.validate()?;
    recipient
        .validate()
        .map_err(|_| CaptureKeyWrapError::InvalidCertificate)?;
    let expected_public = device_encryption_public_key(signing_seed);
    if recipient.device_id != wrapped.recipient_device_id
        || recipient.certificate_id != wrapped.recipient_certificate_id
        || hex(&expected_public) != recipient.device_encryption_public_key
        || SemanticHash(*blake3::hash(&expected_public).as_bytes())
            != wrapped.recipient_encryption_key_hash
    {
        return Err(CaptureKeyWrapError::WrongRecipient);
    }
    let ephemeral = decode_hex::<32>(&wrapped.ephemeral_public_key)
        .ok_or(CaptureKeyWrapError::InvalidEnvelope)?;
    let secret = device_encryption_secret(signing_seed);
    let shared = MontgomeryPoint(ephemeral).mul_clamped(*secret).to_bytes();
    if shared == [0; 32] {
        return Err(CaptureKeyWrapError::Crypto);
    }
    let aad = key_wrap_aad(wrapped)?;
    let nonce_bytes = key_wrap_nonce(wrapped);
    let nonce =
        XNonce::try_from(nonce_bytes.as_slice()).map_err(|_| CaptureKeyWrapError::Crypto)?;
    let plaintext = Zeroizing::new(
        wrap_cipher(shared)?
            .decrypt(
                &nonce,
                Payload {
                    msg: &wrapped.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| CaptureKeyWrapError::Integrity)?,
    );
    let key =
        <[u8; 32]>::try_from(plaintext.as_slice()).map_err(|_| CaptureKeyWrapError::Integrity)?;
    Ok(CaptureTransferKey::new(key))
}

#[derive(Serialize)]
struct CaptureKeyWrapAad<'a> {
    domain: &'static str,
    schema_version: u16,
    recipient_device_id: &'a DeviceId,
    recipient_certificate_id: &'a CertificateId,
    recipient_encryption_key_hash: SemanticHash,
    transfer_id: &'a poche_protocol::CaptureTransferId,
    request_hash: SemanticHash,
    ephemeral_public_key: &'a str,
}

fn envelope_without_ciphertext(
    recipient: &DeviceCertificateWire,
    transfer_id: poche_protocol::CaptureTransferId,
    request_hash: SemanticHash,
    ephemeral_public: [u8; 32],
) -> WrappedCaptureTransferKey {
    let recipient_public = decode_hex::<32>(&recipient.device_encryption_public_key)
        .expect("validated certificate encryption key is exact hexadecimal");
    WrappedCaptureTransferKey {
        schema_version: KEY_WRAP_SCHEMA_VERSION_V1,
        recipient_device_id: recipient.device_id.clone(),
        recipient_certificate_id: recipient.certificate_id.clone(),
        recipient_encryption_key_hash: SemanticHash(*blake3::hash(&recipient_public).as_bytes()),
        transfer_id,
        request_hash,
        ephemeral_public_key: hex(&ephemeral_public),
        ciphertext: Vec::new(),
    }
}

fn seal_content_key(
    envelope: &WrappedCaptureTransferKey,
    shared: [u8; 32],
    content_key: &[u8; 32],
) -> Result<Vec<u8>, CaptureKeyWrapError> {
    let aad = key_wrap_aad(envelope)?;
    let nonce_bytes = key_wrap_nonce(envelope);
    let nonce =
        XNonce::try_from(nonce_bytes.as_slice()).map_err(|_| CaptureKeyWrapError::Crypto)?;
    wrap_cipher(shared)?
        .encrypt(
            &nonce,
            Payload {
                msg: content_key,
                aad: &aad,
            },
        )
        .map_err(|_| CaptureKeyWrapError::Crypto)
}

fn key_wrap_aad(wrapped: &WrappedCaptureTransferKey) -> Result<Vec<u8>, CaptureKeyWrapError> {
    serde_json::to_vec(&CaptureKeyWrapAad {
        domain: "poche/capture-transfer-key-wrap/v1",
        schema_version: wrapped.schema_version,
        recipient_device_id: &wrapped.recipient_device_id,
        recipient_certificate_id: &wrapped.recipient_certificate_id,
        recipient_encryption_key_hash: wrapped.recipient_encryption_key_hash,
        transfer_id: &wrapped.transfer_id,
        request_hash: wrapped.request_hash,
        ephemeral_public_key: &wrapped.ephemeral_public_key,
    })
    .map_err(|_| CaptureKeyWrapError::InvalidEnvelope)
}

fn key_wrap_nonce(wrapped: &WrappedCaptureTransferKey) -> [u8; 24] {
    let mut hasher = blake3::Hasher::new_derive_key("poche/capture-transfer-key-wrap-nonce/v1");
    hasher.update(wrapped.transfer_id.as_str().as_bytes());
    hasher.update(&wrapped.request_hash.0);
    hasher.update(wrapped.recipient_device_id.as_str().as_bytes());
    hasher.update(wrapped.ephemeral_public_key.as_bytes());
    let mut nonce = [0; 24];
    nonce.copy_from_slice(&hasher.finalize().as_bytes()[..24]);
    nonce
}

fn wrap_cipher(shared: [u8; 32]) -> Result<XChaCha20Poly1305, CaptureKeyWrapError> {
    let key = blake3::derive_key("poche/capture-transfer-key-wrap-aead/v1", &shared);
    XChaCha20Poly1305::new_from_slice(&key).map_err(|_| CaptureKeyWrapError::Crypto)
}

fn device_encryption_secret(signing_seed: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(blake3::derive_key(
        "poche/device-encryption-subkey/v1",
        signing_seed,
    ))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(value.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(output)
}

/// Stable key-wrapping failure with no secret material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKeyWrapError {
    InvalidCertificate,
    InvalidEnvelope,
    WrongRecipient,
    RandomUnavailable,
    Crypto,
    Integrity,
}

impl fmt::Display for CaptureKeyWrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCertificate => "capture recipient certificate is invalid",
            Self::InvalidEnvelope => "wrapped capture key is invalid",
            Self::WrongRecipient => "wrapped capture key names another device",
            Self::RandomUnavailable => "capture key randomness is unavailable",
            Self::Crypto => "capture key wrapping failed",
            Self::Integrity => "wrapped capture key failed integrity validation",
        })
    }
}

impl std::error::Error for CaptureKeyWrapError {}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CertificateId, DeviceCapabilityWire, DeviceCustodyWire, PrincipalId,
        REPLICATION_SCHEMA_VERSION_V1, REPLICATION_SIGNATURE_DOMAIN_V1, SignatureAlgorithm,
        SignatureBytes, SignatureIntent, UnsignedDeviceCertificateWire,
    };

    use super::*;

    fn certificate(seed: &[u8; 32], device: &str) -> DeviceCertificateWire {
        let short_device = device.get(..8).unwrap();
        UnsignedDeviceCertificateWire {
            schema_version: REPLICATION_SCHEMA_VERSION_V1,
            certificate_id: CertificateId::new(format!("capture-wrap-{short_device}")).unwrap(),
            player_id: PrincipalId::new("11".repeat(32)).unwrap(),
            device_id: DeviceId::new(device.to_owned()).unwrap(),
            device_signing_public_key: device.to_owned(),
            device_encryption_public_key: hex(&device_encryption_public_key(seed)),
            sequence: 1,
            valid_from_membership_epoch: 1,
            valid_through_membership_epoch: None,
            capabilities: vec![DeviceCapabilityWire::RequestCapture],
            custody: DeviceCustodyWire::NativeLocal,
            signature_intent: SignatureIntent {
                domain_version: REPLICATION_SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: PrincipalId::new("11".repeat(32)).unwrap(),
            },
        }
        .attach_signature(SignatureBytes::new("00".repeat(64)).unwrap())
        .unwrap()
    }

    #[test]
    fn content_key_is_exact_recipient_only_and_tamper_evident() {
        let alice_seed = [7; 32];
        let bob_seed = [8; 32];
        let alice = certificate(&alice_seed, &"22".repeat(32));
        let bob = certificate(&bob_seed, &"33".repeat(32));
        let transfer = poche_protocol::CaptureTransferId::new("wrapped-key-transfer").unwrap();
        let request_hash = SemanticHash([9; 32]);
        let (sender_key, wrapped) =
            generate_wrapped_capture_transfer_key(&alice, transfer, request_hash).unwrap();
        let receiver_key =
            open_wrapped_capture_transfer_key(&wrapped, &alice, &alice_seed).unwrap();
        assert!(format!("{sender_key:?}").contains("redacted"));
        assert!(format!("{receiver_key:?}").contains("redacted"));
        assert!(format!("{wrapped:?}").contains("encrypted-content-key"));
        assert!(
            !serde_json::to_string(&wrapped)
                .unwrap()
                .contains(&hex(&alice_seed))
        );
        assert!(matches!(
            open_wrapped_capture_transfer_key(&wrapped, &bob, &bob_seed),
            Err(CaptureKeyWrapError::WrongRecipient)
        ));
        let mut tampered = wrapped;
        tampered.ciphertext[0] ^= 1;
        assert!(matches!(
            open_wrapped_capture_transfer_key(&tampered, &alice, &alice_seed),
            Err(CaptureKeyWrapError::Integrity)
        ));
    }
}

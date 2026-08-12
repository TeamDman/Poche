use std::{collections::BTreeMap, fmt};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use poche_protocol::{
    CaptureTransferDescriptorWire, CaptureTransferId, MAX_CAPTURE_ARTIFACT_BYTES,
    MAX_CAPTURE_TRANSFER_CHUNK_BYTES, SemanticHash,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const AEAD_TAG_BYTES: usize = 16;

/// Protected per-transfer content key. It is neither cloneable nor
/// serializable, its debug output is redacted, and its bytes zero on drop.
pub struct CaptureTransferKey(Zeroizing<[u8; 32]>);

impl CaptureTransferKey {
    /// Wrap a cryptographically random key unique to this transfer ID. Reusing
    /// key bytes with the same transfer ID is forbidden because chunk nonces
    /// are deterministic within one transfer.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn cipher(&self) -> Result<XChaCha20Poly1305, CaptureTransferError> {
        XChaCha20Poly1305::new_from_slice(self.0.as_ref()).map_err(|_| CaptureTransferError::Crypto)
    }
}

impl fmt::Debug for CaptureTransferKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CaptureTransferKey(<redacted>)")
    }
}

/// One bounded encrypted chunk. The signed provider response owns the
/// descriptor; AEAD associated data binds each chunk to it and its request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedCaptureChunk {
    pub transfer_id: CaptureTransferId,
    pub request_hash: SemanticHash,
    pub chunk_index: u32,
    pub chunk_count: u32,
    pub plaintext_length: u32,
    pub ciphertext: Vec<u8>,
}

/// Bounded sender with explicit acknowledgement credit.
pub struct CaptureTransferSender {
    descriptor: CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
    expires_at_unix_ms: u64,
    bytes: Vec<u8>,
    key: CaptureTransferKey,
    next_index: u32,
    outstanding: BTreeMap<u32, SemanticHash>,
    credit_chunks: usize,
    cancelled: bool,
}

impl CaptureTransferSender {
    pub fn new(
        descriptor: CaptureTransferDescriptorWire,
        request_hash: SemanticHash,
        expires_at_unix_ms: u64,
        bytes: Vec<u8>,
        key: CaptureTransferKey,
        credit_chunks: usize,
    ) -> Result<Self, CaptureTransferError> {
        descriptor
            .validate()
            .map_err(|_| CaptureTransferError::InvalidDescriptor)?;
        if expires_at_unix_ms == 0
            || credit_chunks == 0
            || credit_chunks > 256
            || u64::try_from(bytes.len()).ok() != Some(descriptor.byte_length)
            || SemanticHash(*blake3::hash(&bytes).as_bytes()) != descriptor.content_hash
        {
            return Err(CaptureTransferError::InvalidDescriptor);
        }
        Ok(Self {
            descriptor,
            request_hash,
            expires_at_unix_ms,
            bytes,
            key,
            next_index: 0,
            outstanding: BTreeMap::new(),
            credit_chunks,
            cancelled: false,
        })
    }

    pub fn next_chunk(
        &mut self,
        now_unix_ms: u64,
    ) -> Result<Option<EncryptedCaptureChunk>, CaptureTransferError> {
        self.ensure_active(now_unix_ms)?;
        if self.outstanding.len() >= self.credit_chunks {
            return Err(CaptureTransferError::Backpressure);
        }
        if self.next_index == self.descriptor.chunk_count {
            return Ok(None);
        }
        let index = self.next_index;
        let chunk_bytes = usize::try_from(self.descriptor.chunk_bytes)
            .map_err(|_| CaptureTransferError::InvalidDescriptor)?;
        let start = usize::try_from(index)
            .map_err(|_| CaptureTransferError::InvalidDescriptor)?
            .checked_mul(chunk_bytes)
            .ok_or(CaptureTransferError::InvalidDescriptor)?;
        let end = start.saturating_add(chunk_bytes).min(self.bytes.len());
        let plaintext = self
            .bytes
            .get(start..end)
            .ok_or(CaptureTransferError::InvalidDescriptor)?;
        let chunk = seal_chunk(
            &self.descriptor,
            self.request_hash,
            index,
            plaintext,
            &self.key,
        )?;
        self.outstanding.insert(
            index,
            SemanticHash(*blake3::hash(&chunk.ciphertext).as_bytes()),
        );
        self.next_index = self.next_index.saturating_add(1);
        Ok(Some(chunk))
    }

    pub fn acknowledge(
        &mut self,
        chunk_index: u32,
        ciphertext_hash: SemanticHash,
    ) -> Result<(), CaptureTransferError> {
        if self.outstanding.get(&chunk_index) != Some(&ciphertext_hash) {
            return Err(CaptureTransferError::InvalidAcknowledgement);
        }
        self.outstanding.remove(&chunk_index);
        Ok(())
    }

    pub fn resume_from(&mut self, next_index: u32) -> Result<(), CaptureTransferError> {
        if !self.outstanding.is_empty() || next_index > self.descriptor.chunk_count {
            return Err(CaptureTransferError::ResumeConflict);
        }
        self.next_index = next_index;
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.bytes.clear();
        self.outstanding.clear();
    }

    fn ensure_active(&self, now_unix_ms: u64) -> Result<(), CaptureTransferError> {
        if self.cancelled {
            Err(CaptureTransferError::Cancelled)
        } else if now_unix_ms > self.expires_at_unix_ms {
            Err(CaptureTransferError::Expired)
        } else {
            Ok(())
        }
    }
}

/// Receiver result for an exact chunk delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureChunkDisposition {
    Accepted { ciphertext_hash: SemanticHash },
    Duplicate { ciphertext_hash: SemanticHash },
}

/// Verification-before-publication receiver. Partial content remains private
/// in memory and is erased on cancellation/drop.
pub struct CaptureTransferReceiver {
    descriptor: CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
    expires_at_unix_ms: u64,
    key: CaptureTransferKey,
    bytes: Zeroizing<Vec<u8>>,
    accepted_ciphertext: BTreeMap<u32, SemanticHash>,
    next_index: u32,
    cancelled: bool,
    complete: bool,
}

impl CaptureTransferReceiver {
    pub fn new(
        descriptor: CaptureTransferDescriptorWire,
        request_hash: SemanticHash,
        expires_at_unix_ms: u64,
        key: CaptureTransferKey,
    ) -> Result<Self, CaptureTransferError> {
        descriptor
            .validate()
            .map_err(|_| CaptureTransferError::InvalidDescriptor)?;
        if expires_at_unix_ms == 0 {
            return Err(CaptureTransferError::InvalidDescriptor);
        }
        Ok(Self {
            descriptor,
            request_hash,
            expires_at_unix_ms,
            key,
            bytes: Zeroizing::new(Vec::new()),
            accepted_ciphertext: BTreeMap::new(),
            next_index: 0,
            cancelled: false,
            complete: false,
        })
    }

    #[must_use]
    pub const fn resume_index(&self) -> u32 {
        self.next_index
    }

    pub fn accept(
        &mut self,
        now_unix_ms: u64,
        chunk: &EncryptedCaptureChunk,
    ) -> Result<CaptureChunkDisposition, CaptureTransferError> {
        self.ensure_active(now_unix_ms)?;
        validate_chunk(chunk, &self.descriptor, self.request_hash)?;
        let ciphertext_hash = SemanticHash(*blake3::hash(&chunk.ciphertext).as_bytes());
        if chunk.chunk_index < self.next_index {
            return if self.accepted_ciphertext.get(&chunk.chunk_index) == Some(&ciphertext_hash) {
                Ok(CaptureChunkDisposition::Duplicate { ciphertext_hash })
            } else {
                Err(CaptureTransferError::ConflictingDuplicate)
            };
        }
        if chunk.chunk_index != self.next_index {
            return Err(CaptureTransferError::OutOfOrder);
        }
        let plaintext = open_chunk(&self.descriptor, self.request_hash, chunk, &self.key)?;
        if plaintext.len() != usize::try_from(chunk.plaintext_length).unwrap_or(usize::MAX)
            || self.bytes.len().saturating_add(plaintext.len())
                > usize::try_from(self.descriptor.byte_length).unwrap_or(usize::MAX)
        {
            return Err(CaptureTransferError::InvalidChunk);
        }
        self.bytes.extend_from_slice(&plaintext);
        self.accepted_ciphertext
            .insert(chunk.chunk_index, ciphertext_hash);
        self.next_index = self.next_index.saturating_add(1);
        Ok(CaptureChunkDisposition::Accepted { ciphertext_hash })
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, CaptureTransferError> {
        if self.cancelled {
            return Err(CaptureTransferError::Cancelled);
        }
        if self.complete {
            return Err(CaptureTransferError::AlreadyComplete);
        }
        if self.next_index != self.descriptor.chunk_count
            || u64::try_from(self.bytes.len()).ok() != Some(self.descriptor.byte_length)
            || SemanticHash(*blake3::hash(&self.bytes).as_bytes()) != self.descriptor.content_hash
        {
            return Err(CaptureTransferError::Integrity);
        }
        self.complete = true;
        Ok(std::mem::take(&mut *self.bytes))
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.bytes.clear();
        self.accepted_ciphertext.clear();
    }

    #[must_use]
    pub fn partial_bytes(&self) -> usize {
        self.bytes.len()
    }

    fn ensure_active(&self, now_unix_ms: u64) -> Result<(), CaptureTransferError> {
        if self.cancelled {
            Err(CaptureTransferError::Cancelled)
        } else if self.complete {
            Err(CaptureTransferError::AlreadyComplete)
        } else if now_unix_ms > self.expires_at_unix_ms {
            Err(CaptureTransferError::Expired)
        } else {
            Ok(())
        }
    }
}

/// Stable redacted private-transfer failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureTransferError {
    InvalidDescriptor,
    InvalidChunk,
    InvalidAcknowledgement,
    Backpressure,
    OutOfOrder,
    ConflictingDuplicate,
    ResumeConflict,
    Expired,
    Cancelled,
    AlreadyComplete,
    Crypto,
    Integrity,
}

impl fmt::Display for CaptureTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidDescriptor => "capture transfer descriptor is invalid",
            Self::InvalidChunk => "capture transfer chunk is invalid",
            Self::InvalidAcknowledgement => "capture transfer acknowledgement is invalid",
            Self::Backpressure => "capture transfer is awaiting acknowledgement credit",
            Self::OutOfOrder => "capture transfer chunk is out of order",
            Self::ConflictingDuplicate => "capture transfer duplicate conflicts",
            Self::ResumeConflict => "capture transfer cannot resume at that position",
            Self::Expired => "capture transfer has expired",
            Self::Cancelled => "capture transfer was cancelled",
            Self::AlreadyComplete => "capture transfer is already complete",
            Self::Crypto => "capture transfer cryptography failed",
            Self::Integrity => "capture transfer integrity check failed",
        })
    }
}

impl std::error::Error for CaptureTransferError {}

fn seal_chunk(
    descriptor: &CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
    index: u32,
    plaintext: &[u8],
    key: &CaptureTransferKey,
) -> Result<EncryptedCaptureChunk, CaptureTransferError> {
    let aad = chunk_aad(
        descriptor,
        request_hash,
        index,
        u32::try_from(plaintext.len()).map_err(|_| CaptureTransferError::InvalidChunk)?,
    )?;
    let nonce = chunk_nonce(&descriptor.transfer_id, request_hash, index);
    let nonce = XNonce::try_from(nonce.as_slice()).map_err(|_| CaptureTransferError::Crypto)?;
    let ciphertext = key
        .cipher()?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| CaptureTransferError::Crypto)?;
    Ok(EncryptedCaptureChunk {
        transfer_id: descriptor.transfer_id.clone(),
        request_hash,
        chunk_index: index,
        chunk_count: descriptor.chunk_count,
        plaintext_length: u32::try_from(plaintext.len())
            .map_err(|_| CaptureTransferError::InvalidChunk)?,
        ciphertext,
    })
}

fn open_chunk(
    descriptor: &CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
    chunk: &EncryptedCaptureChunk,
    key: &CaptureTransferKey,
) -> Result<Vec<u8>, CaptureTransferError> {
    let aad = chunk_aad(
        descriptor,
        request_hash,
        chunk.chunk_index,
        chunk.plaintext_length,
    )?;
    let nonce = chunk_nonce(&descriptor.transfer_id, request_hash, chunk.chunk_index);
    let nonce = XNonce::try_from(nonce.as_slice()).map_err(|_| CaptureTransferError::Crypto)?;
    key.cipher()?
        .decrypt(
            &nonce,
            Payload {
                msg: &chunk.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| CaptureTransferError::Integrity)
}

fn validate_chunk(
    chunk: &EncryptedCaptureChunk,
    descriptor: &CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
) -> Result<(), CaptureTransferError> {
    let maximum_plaintext =
        usize::try_from(descriptor.chunk_bytes).map_err(|_| CaptureTransferError::InvalidChunk)?;
    if chunk.transfer_id != descriptor.transfer_id
        || chunk.request_hash != request_hash
        || chunk.chunk_count != descriptor.chunk_count
        || chunk.chunk_index >= descriptor.chunk_count
        || chunk.plaintext_length == 0
        || chunk.plaintext_length > descriptor.chunk_bytes
        || chunk.ciphertext.len()
            != usize::try_from(chunk.plaintext_length)
                .map_err(|_| CaptureTransferError::InvalidChunk)?
                .saturating_add(AEAD_TAG_BYTES)
        || chunk.ciphertext.len() > maximum_plaintext.saturating_add(AEAD_TAG_BYTES)
    {
        return Err(CaptureTransferError::InvalidChunk);
    }
    Ok(())
}

#[derive(Serialize)]
struct ChunkAssociatedData<'a> {
    domain: &'static str,
    transfer_id: &'a CaptureTransferId,
    request_hash: SemanticHash,
    content_hash: SemanticHash,
    byte_length: u64,
    chunk_bytes: u32,
    chunk_count: u32,
    chunk_index: u32,
    plaintext_length: u32,
}

fn chunk_aad(
    descriptor: &CaptureTransferDescriptorWire,
    request_hash: SemanticHash,
    chunk_index: u32,
    plaintext_length: u32,
) -> Result<Vec<u8>, CaptureTransferError> {
    serde_json::to_vec(&ChunkAssociatedData {
        domain: "poche/capture-transfer-chunk/v1",
        transfer_id: &descriptor.transfer_id,
        request_hash,
        content_hash: descriptor.content_hash,
        byte_length: descriptor.byte_length,
        chunk_bytes: descriptor.chunk_bytes,
        chunk_count: descriptor.chunk_count,
        chunk_index,
        plaintext_length,
    })
    .map_err(|_| CaptureTransferError::InvalidChunk)
}

fn chunk_nonce(
    transfer_id: &CaptureTransferId,
    request_hash: SemanticHash,
    chunk_index: u32,
) -> [u8; 24] {
    let mut hasher = blake3::Hasher::new_derive_key("poche/capture-transfer-nonce/v1");
    hasher.update(transfer_id.as_str().as_bytes());
    hasher.update(&request_hash.0);
    hasher.update(&chunk_index.to_be_bytes());
    let mut nonce = [0; 24];
    nonce.copy_from_slice(&hasher.finalize().as_bytes()[..24]);
    nonce
}

/// Build the exact bounded transfer descriptor for content.
pub fn capture_transfer_descriptor(
    transfer_id: CaptureTransferId,
    bytes: &[u8],
    chunk_bytes: u32,
) -> Result<CaptureTransferDescriptorWire, CaptureTransferError> {
    let byte_length =
        u64::try_from(bytes.len()).map_err(|_| CaptureTransferError::InvalidDescriptor)?;
    if byte_length == 0
        || byte_length > MAX_CAPTURE_ARTIFACT_BYTES
        || chunk_bytes == 0
        || chunk_bytes > MAX_CAPTURE_TRANSFER_CHUNK_BYTES
    {
        return Err(CaptureTransferError::InvalidDescriptor);
    }
    let chunk_count = u32::try_from(byte_length.div_ceil(u64::from(chunk_bytes)))
        .map_err(|_| CaptureTransferError::InvalidDescriptor)?;
    let descriptor = CaptureTransferDescriptorWire {
        transfer_id,
        byte_length,
        chunk_bytes,
        chunk_count,
        content_hash: SemanticHash(*blake3::hash(bytes).as_bytes()),
    };
    descriptor
        .validate()
        .map_err(|_| CaptureTransferError::InvalidDescriptor)?;
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapturePipeline, CaptureQualification, CaptureSurfaceMetadata, PrivateMarker,
        RawCaptureArtifact, RawCaptureBundle,
    };
    use poche_protocol::{
        CaptureArtifactId, CaptureProviderKindWire, CaptureRepresentationWire, CaptureViewportWire,
    };

    fn bytes() -> Vec<u8> {
        (0..3 * 1024 * 1024 + 17)
            .map(|index| b'a' + u8::try_from(index % 26).unwrap())
            .collect()
    }

    #[test]
    fn multi_megabyte_transfer_enforces_credit_resume_dedup_and_integrity() {
        let source = bytes();
        let descriptor = capture_transfer_descriptor(
            CaptureTransferId::new("large-transfer").unwrap(),
            &source,
            MAX_CAPTURE_TRANSFER_CHUNK_BYTES,
        )
        .unwrap();
        let request_hash = SemanticHash([4; 32]);
        let mut sender = CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            20_000,
            source.clone(),
            CaptureTransferKey::new([7; 32]),
            2,
        )
        .unwrap();
        let mut receiver = CaptureTransferReceiver::new(
            descriptor.clone(),
            request_hash,
            20_000,
            CaptureTransferKey::new([7; 32]),
        )
        .unwrap();

        let first = sender.next_chunk(10_000).unwrap().unwrap();
        let second = sender.next_chunk(10_000).unwrap().unwrap();
        assert_ne!(
            first
                .ciphertext
                .get(..usize::try_from(first.plaintext_length).unwrap()),
            source.get(..usize::try_from(first.plaintext_length).unwrap())
        );
        assert_eq!(
            sender.next_chunk(10_000),
            Err(CaptureTransferError::Backpressure)
        );
        let CaptureChunkDisposition::Accepted { ciphertext_hash } =
            receiver.accept(10_000, &first).unwrap()
        else {
            unreachable!();
        };
        sender
            .acknowledge(first.chunk_index, ciphertext_hash)
            .unwrap();
        assert_eq!(
            receiver.accept(10_000, &first),
            Ok(CaptureChunkDisposition::Duplicate { ciphertext_hash })
        );
        let CaptureChunkDisposition::Accepted { ciphertext_hash } =
            receiver.accept(10_000, &second).unwrap()
        else {
            unreachable!();
        };
        sender
            .acknowledge(second.chunk_index, ciphertext_hash)
            .unwrap();
        assert_eq!(receiver.finish(), Err(CaptureTransferError::Integrity));

        sender.resume_from(receiver.resume_index()).unwrap();
        while let Some(chunk) = sender.next_chunk(10_000).unwrap() {
            let CaptureChunkDisposition::Accepted { ciphertext_hash } =
                receiver.accept(10_000, &chunk).unwrap()
            else {
                unreachable!();
            };
            sender
                .acknowledge(chunk.chunk_index, ciphertext_hash)
                .unwrap();
        }
        let verified_bytes = receiver.finish().unwrap();
        assert_eq!(verified_bytes, source);
        publish_verified_bytes(&verified_bytes, descriptor.content_hash);
        assert_eq!(
            receiver.finish(),
            Err(CaptureTransferError::AlreadyComplete)
        );
    }

    fn publish_verified_bytes(bytes: &[u8], expected_hash: SemanticHash) {
        let root = tempfile::tempdir().unwrap();
        let persisted = CapturePipeline::new(root.path())
            .with_private_markers(vec![PrivateMarker::new(b"not-present".to_vec())])
            .persist(&RawCaptureBundle {
                figure_id: "transferred-evidence".to_owned(),
                caption: "Verified private transfer".to_owned(),
                captured_revision: 9,
                projection_hash: SemanticHash([6; 32]),
                scene_hash: None,
                surface: CaptureSurfaceMetadata {
                    provider_kind: CaptureProviderKindWire::HeadlessSemantic,
                    viewport: CaptureViewportWire {
                        width_pixels: 1,
                        height_pixels: 1,
                    },
                    framebuffer_width: 1,
                    framebuffer_height: 1,
                    scale_milli: 1_000,
                    camera: None,
                },
                qualification: CaptureQualification::RuntimeGenerated,
                cancelled: false,
                artifacts: vec![RawCaptureArtifact {
                    artifact_id: CaptureArtifactId::new("transferred-html").unwrap(),
                    representation: CaptureRepresentationWire::SemanticHtml,
                    media_type: "text/html; charset=utf-8".to_owned(),
                    bytes: bytes.to_vec(),
                    expected_source_hash: Some(expected_hash),
                }],
            })
            .unwrap();
        assert_eq!(
            persisted.manifest.entries[0].byte_length,
            u64::try_from(bytes.len()).unwrap()
        );
    }

    #[test]
    fn tamper_expiry_wrong_key_and_cancel_fail_without_publishable_bytes() {
        let source = vec![9; 50_000];
        let descriptor = capture_transfer_descriptor(
            CaptureTransferId::new("failure-transfer").unwrap(),
            &source,
            24_000,
        )
        .unwrap();
        let request_hash = SemanticHash([5; 32]);
        let mut sender = CaptureTransferSender::new(
            descriptor.clone(),
            request_hash,
            20_000,
            source,
            CaptureTransferKey::new([8; 32]),
            1,
        )
        .unwrap();
        let chunk = sender.next_chunk(10_000).unwrap().unwrap();
        let mut wrong_key = CaptureTransferReceiver::new(
            descriptor.clone(),
            request_hash,
            20_000,
            CaptureTransferKey::new([3; 32]),
        )
        .unwrap();
        assert_eq!(
            wrong_key.accept(10_000, &chunk),
            Err(CaptureTransferError::Integrity)
        );
        assert_eq!(wrong_key.partial_bytes(), 0);

        let mut receiver = CaptureTransferReceiver::new(
            descriptor,
            request_hash,
            20_000,
            CaptureTransferKey::new([8; 32]),
        )
        .unwrap();
        assert_eq!(
            receiver.accept(20_001, &chunk),
            Err(CaptureTransferError::Expired)
        );
        let mut tampered = chunk;
        tampered.ciphertext[0] ^= 1;
        assert_eq!(
            receiver.accept(10_000, &tampered),
            Err(CaptureTransferError::Integrity)
        );
        assert_eq!(receiver.partial_bytes(), 0);
        receiver.cancel();
        assert_eq!(receiver.finish(), Err(CaptureTransferError::Cancelled));
        assert_eq!(receiver.partial_bytes(), 0);
        assert!(format!("{:?}", CaptureTransferKey::new([42; 32])).contains("redacted"));
    }
}

use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{PROTOCOL_VERSION_V1, PrincipalId};

use crate::RENDEZVOUS_SCHEMA_VERSION_V1;

const CODE_PREFIX: &str = "p3-";
const CODE_MAGIC: &[u8; 2] = b"PC";
const CODE_VERSION: u8 = 1;
const CHECKSUM_BYTES: usize = 8;
const INVITE_SECRET_BYTES: usize = 32;
const HOST_KEY_BYTES: usize = 32;
const HEADER_BYTES: usize = 17;
const MAX_RECORD_KEY_BYTES: usize = 96;
const MAX_ROOM_CODE_BYTES: usize = 256;

/// Network selected by a room locator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RoomNetwork {
    VeilidPublic = 1,
    VeilidLocal = 2,
}

impl TryFrom<u8> for RoomNetwork {
    type Error = RoomCodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::VeilidPublic),
            2 => Ok(Self::VeilidLocal),
            _ => Err(RoomCodeError::UnsupportedNetwork),
        }
    }
}

/// Stable, non-diagnostic room-code failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomCodeError {
    InvalidFormat,
    UnsupportedVersion,
    UnsupportedNetwork,
    ProtocolMismatch,
    SchemaMismatch,
    InvalidRecordKey,
    InvalidHostPrincipal,
    ChecksumMismatch,
    Expired,
    RandomUnavailable,
}

/// A compact secret room code.
///
/// The full encrypted DHT record key and invite secret are deliberately absent
/// from diagnostics and serialization traits.
pub struct RoomCode {
    network: RoomNetwork,
    expires_at_unix_ms: u64,
    encrypted_record_key: Vec<u8>,
    host_principal: PrincipalId,
    invite_secret: [u8; INVITE_SECRET_BYTES],
}

impl fmt::Debug for RoomCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomCode(<redacted>)")
    }
}

impl Drop for RoomCode {
    fn drop(&mut self) {
        self.encrypted_record_key.fill(0);
        self.invite_secret.fill(0);
    }
}

impl RoomCode {
    /// Issue a one-time/expiring code for an encrypted Veilid DHT record key.
    ///
    /// # Errors
    ///
    /// Rejects invalid bounds/identity, a nonfuture expiry, or unavailable OS
    /// randomness.
    pub fn issue(
        network: RoomNetwork,
        encrypted_record_key: &str,
        host_principal: PrincipalId,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<Self, RoomCodeError> {
        let mut invite_secret = [0_u8; INVITE_SECRET_BYTES];
        getrandom::fill(&mut invite_secret).map_err(|_| RoomCodeError::RandomUnavailable)?;
        Self::from_secret(
            network,
            encrypted_record_key,
            host_principal,
            expires_at_unix_ms,
            now_unix_ms,
            invite_secret,
        )
    }

    fn from_secret(
        network: RoomNetwork,
        encrypted_record_key: &str,
        host_principal: PrincipalId,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
        invite_secret: [u8; INVITE_SECRET_BYTES],
    ) -> Result<Self, RoomCodeError> {
        validate_record_key(encrypted_record_key.as_bytes())?;
        decode_hex_32(host_principal.as_str())?;
        if expires_at_unix_ms <= now_unix_ms {
            return Err(RoomCodeError::Expired);
        }
        Ok(Self {
            network,
            expires_at_unix_ms,
            encrypted_record_key: encrypted_record_key.as_bytes().to_vec(),
            host_principal,
            invite_secret,
        })
    }

    /// Decode and validate a canonical room code at authority time.
    ///
    /// # Errors
    ///
    /// Fails closed for malformed, noncanonical, unsupported, corrupt, or
    /// expired input without returning any input fragment in the error.
    pub fn decode(text: &str, now_unix_ms: u64) -> Result<Self, RoomCodeError> {
        if text.len() > MAX_ROOM_CODE_BYTES || !text.starts_with(CODE_PREFIX) {
            return Err(RoomCodeError::InvalidFormat);
        }
        let mut decoded = SecretBuffer(
            BASE64URL_NOPAD
                .decode(&text.as_bytes()[CODE_PREFIX.len()..])
                .map_err(|_| RoomCodeError::InvalidFormat)?,
        );
        let bytes = &mut decoded.0;
        if bytes.len() < HEADER_BYTES + HOST_KEY_BYTES + INVITE_SECRET_BYTES + CHECKSUM_BYTES
            || &bytes[..2] != CODE_MAGIC
        {
            return Err(RoomCodeError::InvalidFormat);
        }
        if bytes[2] != CODE_VERSION {
            return Err(RoomCodeError::UnsupportedVersion);
        }
        let network = RoomNetwork::try_from(bytes[3])?;
        if u16::from_le_bytes([bytes[4], bytes[5]]) != PROTOCOL_VERSION_V1 {
            return Err(RoomCodeError::ProtocolMismatch);
        }
        if u16::from_le_bytes([bytes[6], bytes[7]]) != RENDEZVOUS_SCHEMA_VERSION_V1 {
            return Err(RoomCodeError::SchemaMismatch);
        }
        let expires_at_unix_ms = u64::from_le_bytes(
            bytes[8..16]
                .try_into()
                .map_err(|_| RoomCodeError::InvalidFormat)?,
        );
        if expires_at_unix_ms <= now_unix_ms {
            return Err(RoomCodeError::Expired);
        }
        let record_key_len = usize::from(bytes[16]);
        let expected_len =
            HEADER_BYTES + record_key_len + HOST_KEY_BYTES + INVITE_SECRET_BYTES + CHECKSUM_BYTES;
        if bytes.len() != expected_len {
            return Err(RoomCodeError::InvalidFormat);
        }
        let checksum_offset = expected_len - CHECKSUM_BYTES;
        if !constant_time_equal(
            &code_checksum(&bytes[..checksum_offset]),
            &bytes[checksum_offset..],
        ) {
            return Err(RoomCodeError::ChecksumMismatch);
        }
        let record_key_end = HEADER_BYTES + record_key_len;
        validate_record_key(&bytes[HEADER_BYTES..record_key_end])?;
        let host_key_end = record_key_end + HOST_KEY_BYTES;
        let host_principal = PrincipalId::new(hex(&bytes[record_key_end..host_key_end]))
            .map_err(|_| RoomCodeError::InvalidHostPrincipal)?;
        let secret_end = host_key_end + INVITE_SECRET_BYTES;
        let invite_secret = bytes[host_key_end..secret_end]
            .try_into()
            .map_err(|_| RoomCodeError::InvalidFormat)?;
        Ok(Self {
            network,
            expires_at_unix_ms,
            encrypted_record_key: bytes[HEADER_BYTES..record_key_end].to_vec(),
            host_principal,
            invite_secret,
        })
    }

    /// Return the network encoded by this code.
    #[must_use]
    pub const fn network(&self) -> RoomNetwork {
        self.network
    }

    /// Return the authority-clock expiry encoded by this code.
    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    /// Return the expected stable host signing principal.
    #[must_use]
    pub fn host_principal(&self) -> &PrincipalId {
        &self.host_principal
    }

    /// Borrow the encrypted Veilid record key only inside an explicit closure.
    ///
    /// # Panics
    ///
    /// Panics only if the constructor/decoder's UTF-8 validation invariant is
    /// violated by an internal defect.
    pub fn with_encrypted_record_key<R>(&self, operation: impl FnOnce(&str) -> R) -> R {
        operation(
            std::str::from_utf8(&self.encrypted_record_key)
                .expect("validated room-code record keys are UTF-8"),
        )
    }

    /// Render the canonical bearer credential into a zeroing text wrapper.
    ///
    /// # Errors
    ///
    /// Fails if a future encoding change would exceed the protocol invite
    /// bound.
    pub fn encode(&self) -> Result<RoomCodeText, RoomCodeError> {
        let host_key = decode_hex_32(self.host_principal.as_str())?;
        let key_len = u8::try_from(self.encrypted_record_key.len())
            .map_err(|_| RoomCodeError::InvalidRecordKey)?;
        let mut binary = SecretBuffer(Vec::with_capacity(
            HEADER_BYTES
                + self.encrypted_record_key.len()
                + HOST_KEY_BYTES
                + INVITE_SECRET_BYTES
                + CHECKSUM_BYTES,
        ));
        binary.0.extend_from_slice(CODE_MAGIC);
        binary.0.push(CODE_VERSION);
        binary.0.push(self.network as u8);
        binary
            .0
            .extend_from_slice(&PROTOCOL_VERSION_V1.to_le_bytes());
        binary
            .0
            .extend_from_slice(&RENDEZVOUS_SCHEMA_VERSION_V1.to_le_bytes());
        binary
            .0
            .extend_from_slice(&self.expires_at_unix_ms.to_le_bytes());
        binary.0.push(key_len);
        binary.0.extend_from_slice(&self.encrypted_record_key);
        binary.0.extend_from_slice(&host_key);
        binary.0.extend_from_slice(&self.invite_secret);
        let checksum = code_checksum(&binary.0);
        binary.0.extend_from_slice(&checksum);
        let mut text = CODE_PREFIX.as_bytes().to_vec();
        text.extend_from_slice(BASE64URL_NOPAD.encode(&binary.0).as_bytes());
        if text.len() > MAX_ROOM_CODE_BYTES {
            text.fill(0);
            return Err(RoomCodeError::InvalidFormat);
        }
        Ok(RoomCodeText(text))
    }
}

/// Canonical text for explicit display or protocol submission.
pub struct RoomCodeText(Vec<u8>);

impl RoomCodeText {
    /// Explicitly expose the bearer credential to a trusted output boundary.
    ///
    /// # Panics
    ///
    /// Panics only if the canonical encoder emits non-ASCII bytes.
    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).expect("room-code encoding is ASCII")
    }
}

impl fmt::Debug for RoomCodeText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomCodeText(<redacted>)")
    }
}

impl Drop for RoomCodeText {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

struct SecretBuffer(Vec<u8>);

impl Drop for SecretBuffer {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

fn validate_record_key(value: &[u8]) -> Result<(), RoomCodeError> {
    if value.is_empty()
        || value.len() > MAX_RECORD_KEY_BYTES
        || !value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
    {
        return Err(RoomCodeError::InvalidRecordKey);
    }
    Ok(())
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], RoomCodeError> {
    if value.len() != 64 {
        return Err(RoomCodeError::InvalidHostPrincipal);
    }
    let mut output = [0_u8; 32];
    for (target, pair) in output.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let high = nibble(pair[0]).ok_or(RoomCodeError::InvalidHostPrincipal)?;
        let low = nibble(pair[1]).ok_or(RoomCodeError::InvalidHostPrincipal)?;
        *target = (high << 4) | low;
    }
    Ok(output)
}

fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn code_checksum(payload: &[u8]) -> [u8; CHECKSUM_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-room-code-v1\0");
    hasher.update(payload);
    hasher.finalize().as_bytes()[..CHECKSUM_BYTES]
        .try_into()
        .expect("checksum slice has a fixed length")
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let maximum = left.len().max(right.len());
    for index in 0..maximum {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD_KEY: &str = "VLD0:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    fn principal(byte: u8) -> PrincipalId {
        PrincipalId::new(format!("{byte:02x}").repeat(32)).unwrap()
    }

    #[test]
    fn code_round_trip_is_compact_canonical_and_redacted() {
        let code = RoomCode::from_secret(
            RoomNetwork::VeilidLocal,
            RECORD_KEY,
            principal(0x2a),
            20_000,
            10_000,
            [7_u8; 32],
        )
        .unwrap();
        let text = code.encode().unwrap();
        assert!(text.expose().len() <= 256);
        assert_eq!(format!("{code:?}"), "RoomCode(<redacted>)");
        assert_eq!(format!("{text:?}"), "RoomCodeText(<redacted>)");
        let decoded = RoomCode::decode(text.expose(), 10_000).unwrap();
        assert_eq!(decoded.network(), RoomNetwork::VeilidLocal);
        assert_eq!(decoded.expires_at_unix_ms(), 20_000);
        assert_eq!(decoded.host_principal(), &principal(0x2a));
        decoded.with_encrypted_record_key(|key| assert_eq!(key, RECORD_KEY));
        assert_eq!(decoded.encode().unwrap().expose(), text.expose());
    }

    #[test]
    fn mutation_expiry_and_noncanonical_inputs_fail_closed() {
        let code = RoomCode::from_secret(
            RoomNetwork::VeilidPublic,
            RECORD_KEY,
            principal(0x11),
            20_000,
            10_000,
            [3_u8; 32],
        )
        .unwrap();
        let text = code.encode().unwrap();
        assert_eq!(
            RoomCode::decode(text.expose(), 20_000).unwrap_err(),
            RoomCodeError::Expired
        );
        let mut mutated = text.expose().as_bytes().to_vec();
        let last = mutated.len() - 1;
        mutated[last] = if mutated[last] == b'A' { b'B' } else { b'A' };
        assert!(matches!(
            RoomCode::decode(std::str::from_utf8(&mutated).unwrap(), 10_000),
            Err(RoomCodeError::ChecksumMismatch | RoomCodeError::InvalidFormat)
        ));
        mutated.fill(0);
        assert_eq!(
            RoomCode::decode("not-a-room-code", 10_000).unwrap_err(),
            RoomCodeError::InvalidFormat
        );
    }
}

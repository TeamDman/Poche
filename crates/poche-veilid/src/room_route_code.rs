// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned route-selecting room credential.
//!
//! Existing `p3-` codes remain byte-for-byte Veilid-v1 credentials. A `p3r-`
//! code selects exactly one initial rendezvous path. After redemption, signed
//! membership records may rotate or add locators without changing player or
//! device authority.

use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use poche_protocol::{PROTOCOL_VERSION_V1, PrincipalId};

use crate::room_code::{decode_hex_32, hex, validate_record_key};
use crate::{RENDEZVOUS_SCHEMA_VERSION_V1, RoomNetwork};

const PREFIX: &str = "p3r-";
const MAGIC: &[u8; 2] = b"PR";
const VERSION: u8 = 1;
const HEADER_BYTES: usize = 17;
const HOST_KEY_BYTES: usize = 32;
const INVITE_SECRET_BYTES: usize = 32;
const CHECKSUM_BYTES: usize = 8;
const MAX_ROUTE_PAYLOAD_BYTES: usize = 98;
const MAX_ORIGIN_BYTES: usize = 64;
const MAX_ROOM_HANDLE_BYTES: usize = 32;
const MAX_CODE_BYTES: usize = 256;

/// Initial rendezvous kind. It is a locator, not application authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RoomRouteKind {
    DirectVeilid = 1,
    GatewayHttps = 2,
    ExplicitLoopbackHttp = 3,
}

impl TryFrom<u8> for RoomRouteKind {
    type Error = RoomRouteCodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::DirectVeilid),
            2 => Ok(Self::GatewayHttps),
            3 => Ok(Self::ExplicitLoopbackHttp),
            _ => Err(RoomRouteCodeError::UnsupportedRoute),
        }
    }
}

/// One replaceable rendezvous hint carried by the credential.
#[derive(Clone, PartialEq, Eq)]
pub enum RoomRouteHint {
    DirectVeilid {
        network: RoomNetwork,
        encrypted_record_key: String,
    },
    GatewayHttps {
        origin: String,
        room_handle: String,
    },
    ExplicitLoopbackHttp {
        origin: String,
        room_handle: String,
    },
}

impl fmt::Debug for RoomRouteHint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RoomRouteHint")
            .field("kind", &self.kind())
            .field("locator", &"<redacted>")
            .finish()
    }
}

impl RoomRouteHint {
    /// Direct public/local Veilid DHT rendezvous.
    ///
    /// # Errors
    ///
    /// Rejects a malformed or oversized encrypted DHT record key.
    pub fn direct_veilid(
        network: RoomNetwork,
        encrypted_record_key: impl Into<String>,
    ) -> Result<Self, RoomRouteCodeError> {
        let encrypted_record_key = encrypted_record_key.into();
        validate_record_key(encrypted_record_key.as_bytes())
            .map_err(|_| RoomRouteCodeError::InvalidLocator)?;
        Ok(Self::DirectVeilid {
            network,
            encrypted_record_key,
        })
    }

    /// Reachable TLS Poche gateway origin plus opaque room handle.
    ///
    /// # Errors
    ///
    /// Rejects non-HTTPS/non-origin URLs or noncanonical handles.
    pub fn gateway_https(
        origin: impl Into<String>,
        room_handle: impl Into<String>,
    ) -> Result<Self, RoomRouteCodeError> {
        let origin = origin.into();
        let room_handle = room_handle.into();
        validate_origin(&origin, true)?;
        validate_handle(&room_handle)?;
        Ok(Self::GatewayHttps {
            origin,
            room_handle,
        })
    }

    /// Explicit insecure-development route restricted to a loopback origin.
    ///
    /// # Errors
    ///
    /// Rejects HTTPS, remote HTTP hosts, non-origin URLs, or invalid handles.
    pub fn explicit_loopback_http(
        origin: impl Into<String>,
        room_handle: impl Into<String>,
    ) -> Result<Self, RoomRouteCodeError> {
        let origin = origin.into();
        let room_handle = room_handle.into();
        validate_origin(&origin, false)?;
        validate_handle(&room_handle)?;
        Ok(Self::ExplicitLoopbackHttp {
            origin,
            room_handle,
        })
    }

    /// Stable route discriminant.
    #[must_use]
    pub const fn kind(&self) -> RoomRouteKind {
        match self {
            Self::DirectVeilid { .. } => RoomRouteKind::DirectVeilid,
            Self::GatewayHttps { .. } => RoomRouteKind::GatewayHttps,
            Self::ExplicitLoopbackHttp { .. } => RoomRouteKind::ExplicitLoopbackHttp,
        }
    }

    fn encode_payload(&self) -> Result<Vec<u8>, RoomRouteCodeError> {
        let mut payload = Vec::with_capacity(MAX_ROUTE_PAYLOAD_BYTES);
        match self {
            Self::DirectVeilid {
                network,
                encrypted_record_key,
            } => {
                payload.push(*network as u8);
                push_short(&mut payload, encrypted_record_key.as_bytes())?;
            }
            Self::GatewayHttps {
                origin,
                room_handle,
            }
            | Self::ExplicitLoopbackHttp {
                origin,
                room_handle,
            } => {
                push_short(&mut payload, origin.as_bytes())?;
                push_short(&mut payload, room_handle.as_bytes())?;
            }
        }
        if payload.len() > MAX_ROUTE_PAYLOAD_BYTES {
            return Err(RoomRouteCodeError::InvalidLocator);
        }
        Ok(payload)
    }

    fn decode(kind: RoomRouteKind, bytes: &[u8]) -> Result<Self, RoomRouteCodeError> {
        let mut cursor = Cursor::new(bytes);
        let result = match kind {
            RoomRouteKind::DirectVeilid => {
                let network = RoomNetwork::try_from(cursor.byte()?)
                    .map_err(|_| RoomRouteCodeError::InvalidLocator)?;
                let key = cursor.short_string(96)?;
                Self::direct_veilid(network, key)?
            }
            RoomRouteKind::GatewayHttps => {
                let origin = cursor.short_string(MAX_ORIGIN_BYTES)?;
                let handle = cursor.short_string(MAX_ROOM_HANDLE_BYTES)?;
                Self::gateway_https(origin, handle)?
            }
            RoomRouteKind::ExplicitLoopbackHttp => {
                let origin = cursor.short_string(MAX_ORIGIN_BYTES)?;
                let handle = cursor.short_string(MAX_ROOM_HANDLE_BYTES)?;
                Self::explicit_loopback_http(origin, handle)?
            }
        };
        cursor.finish()?;
        Ok(result)
    }
}

/// Stable failures that never echo secret input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomRouteCodeError {
    InvalidFormat,
    UnsupportedVersion,
    UnsupportedRoute,
    ProtocolMismatch,
    SchemaMismatch,
    InvalidLocator,
    InvalidHostPrincipal,
    ChecksumMismatch,
    Expired,
    RandomUnavailable,
}

/// One expiring bearer invitation plus one replaceable initial locator.
pub struct RoomRouteCode {
    expires_at_unix_ms: u64,
    route: RoomRouteHint,
    host_principal: PrincipalId,
    invite_secret: [u8; INVITE_SECRET_BYTES],
}

impl fmt::Debug for RoomRouteCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomRouteCode(<redacted>)")
    }
}

impl Drop for RoomRouteCode {
    fn drop(&mut self) {
        match &mut self.route {
            RoomRouteHint::DirectVeilid {
                encrypted_record_key,
                ..
            } => unsafe_string_zero(encrypted_record_key),
            RoomRouteHint::GatewayHttps {
                origin,
                room_handle,
            }
            | RoomRouteHint::ExplicitLoopbackHttp {
                origin,
                room_handle,
            } => {
                unsafe_string_zero(origin);
                unsafe_string_zero(room_handle);
            }
        }
        self.invite_secret.fill(0);
    }
}

impl RoomRouteCode {
    /// Issue with OS randomness.
    ///
    /// # Errors
    ///
    /// Rejects an invalid host/expiry/route or unavailable OS randomness.
    pub fn issue(
        route: RoomRouteHint,
        host_principal: PrincipalId,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<Self, RoomRouteCodeError> {
        let mut invite_secret = [0_u8; INVITE_SECRET_BYTES];
        getrandom::fill(&mut invite_secret).map_err(|_| RoomRouteCodeError::RandomUnavailable)?;
        Self::from_secret(
            route,
            host_principal,
            expires_at_unix_ms,
            now_unix_ms,
            invite_secret,
        )
    }

    fn from_secret(
        route: RoomRouteHint,
        host_principal: PrincipalId,
        expires_at_unix_ms: u64,
        now_unix_ms: u64,
        invite_secret: [u8; INVITE_SECRET_BYTES],
    ) -> Result<Self, RoomRouteCodeError> {
        decode_hex_32(host_principal.as_str())
            .map_err(|_| RoomRouteCodeError::InvalidHostPrincipal)?;
        route.encode_payload()?;
        if expires_at_unix_ms <= now_unix_ms {
            return Err(RoomRouteCodeError::Expired);
        }
        Ok(Self {
            expires_at_unix_ms,
            route,
            host_principal,
            invite_secret,
        })
    }

    /// Decode one exact canonical `p3r-` credential.
    ///
    /// # Errors
    ///
    /// Fails closed for malformed, noncanonical, corrupt, expired, or
    /// unsupported input without echoing a locator or secret.
    pub fn decode(text: &str, now_unix_ms: u64) -> Result<Self, RoomRouteCodeError> {
        if text.len() > MAX_CODE_BYTES || !text.starts_with(PREFIX) {
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        let mut binary = SecretBuffer(
            BASE64URL_NOPAD
                .decode(&text.as_bytes()[PREFIX.len()..])
                .map_err(|_| RoomRouteCodeError::InvalidFormat)?,
        );
        let bytes = &mut binary.0;
        if bytes.len() < HEADER_BYTES + HOST_KEY_BYTES + INVITE_SECRET_BYTES + CHECKSUM_BYTES
            || &bytes[..2] != MAGIC
        {
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        if bytes[2] != VERSION {
            return Err(RoomRouteCodeError::UnsupportedVersion);
        }
        let kind = RoomRouteKind::try_from(bytes[3])?;
        if u16::from_le_bytes([bytes[4], bytes[5]]) != PROTOCOL_VERSION_V1 {
            return Err(RoomRouteCodeError::ProtocolMismatch);
        }
        if u16::from_le_bytes([bytes[6], bytes[7]]) != RENDEZVOUS_SCHEMA_VERSION_V1 {
            return Err(RoomRouteCodeError::SchemaMismatch);
        }
        let expires_at_unix_ms = u64::from_le_bytes(
            bytes[8..16]
                .try_into()
                .map_err(|_| RoomRouteCodeError::InvalidFormat)?,
        );
        if expires_at_unix_ms <= now_unix_ms {
            return Err(RoomRouteCodeError::Expired);
        }
        let route_len = usize::from(bytes[16]);
        if route_len > MAX_ROUTE_PAYLOAD_BYTES {
            return Err(RoomRouteCodeError::InvalidLocator);
        }
        let expected =
            HEADER_BYTES + route_len + HOST_KEY_BYTES + INVITE_SECRET_BYTES + CHECKSUM_BYTES;
        if bytes.len() != expected {
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        let checksum_offset = expected - CHECKSUM_BYTES;
        if !constant_time_equal(
            &checksum(&bytes[..checksum_offset]),
            &bytes[checksum_offset..],
        ) {
            return Err(RoomRouteCodeError::ChecksumMismatch);
        }
        let route_end = HEADER_BYTES + route_len;
        let route = RoomRouteHint::decode(kind, &bytes[HEADER_BYTES..route_end])?;
        let host_end = route_end + HOST_KEY_BYTES;
        let host_principal = PrincipalId::new(hex(&bytes[route_end..host_end]))
            .map_err(|_| RoomRouteCodeError::InvalidHostPrincipal)?;
        let secret_end = host_end + INVITE_SECRET_BYTES;
        let invite_secret = bytes[host_end..secret_end]
            .try_into()
            .map_err(|_| RoomRouteCodeError::InvalidFormat)?;
        let result = Self {
            expires_at_unix_ms,
            route,
            host_principal,
            invite_secret,
        };
        if result.encode()?.expose() != text {
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        Ok(result)
    }

    /// Initial locator kind, safe to display without its value.
    #[must_use]
    pub const fn route_kind(&self) -> RoomRouteKind {
        self.route.kind()
    }

    /// Expiry enforced by the redeeming authority.
    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    /// Stable expected host principal. It is not inferred from the locator.
    #[must_use]
    pub fn host_principal(&self) -> &PrincipalId {
        &self.host_principal
    }

    /// Borrow the locator only inside an explicit handling boundary.
    pub fn with_route_hint<R>(&self, operation: impl FnOnce(&RoomRouteHint) -> R) -> R {
        operation(&self.route)
    }

    /// Encode into a zeroing text wrapper compatible with the invite bound.
    ///
    /// # Errors
    ///
    /// Rejects a future field expansion that exceeds the exact payload/code bound.
    pub fn encode(&self) -> Result<RoomRouteCodeText, RoomRouteCodeError> {
        let payload = self.route.encode_payload()?;
        let route_len =
            u8::try_from(payload.len()).map_err(|_| RoomRouteCodeError::InvalidLocator)?;
        let host_key = decode_hex_32(self.host_principal.as_str())
            .map_err(|_| RoomRouteCodeError::InvalidHostPrincipal)?;
        let mut binary = SecretBuffer(Vec::with_capacity(
            HEADER_BYTES + payload.len() + HOST_KEY_BYTES + INVITE_SECRET_BYTES + CHECKSUM_BYTES,
        ));
        binary.0.extend_from_slice(MAGIC);
        binary.0.push(VERSION);
        binary.0.push(self.route.kind() as u8);
        binary
            .0
            .extend_from_slice(&PROTOCOL_VERSION_V1.to_le_bytes());
        binary
            .0
            .extend_from_slice(&RENDEZVOUS_SCHEMA_VERSION_V1.to_le_bytes());
        binary
            .0
            .extend_from_slice(&self.expires_at_unix_ms.to_le_bytes());
        binary.0.push(route_len);
        binary.0.extend_from_slice(&payload);
        binary.0.extend_from_slice(&host_key);
        binary.0.extend_from_slice(&self.invite_secret);
        let checksum = checksum(&binary.0);
        binary.0.extend_from_slice(&checksum);
        let mut text = PREFIX.as_bytes().to_vec();
        text.extend_from_slice(BASE64URL_NOPAD.encode(&binary.0).as_bytes());
        if text.len() > MAX_CODE_BYTES {
            text.fill(0);
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        Ok(RoomRouteCodeText(text))
    }
}

/// Canonical secret text exposed only to explicit output/ingress boundaries.
pub struct RoomRouteCodeText(Vec<u8>);

impl RoomRouteCodeText {
    /// Explicitly expose canonical ASCII.
    ///
    /// # Panics
    ///
    /// Panics only for an internal encoder invariant violation.
    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.0).expect("route-code encoding is ASCII")
    }
}

impl fmt::Debug for RoomRouteCodeText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomRouteCodeText(<redacted>)")
    }
}

impl Drop for RoomRouteCodeText {
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

struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn byte(&mut self) -> Result<u8, RoomRouteCodeError> {
        let Some((&byte, remaining)) = self.remaining.split_first() else {
            return Err(RoomRouteCodeError::InvalidFormat);
        };
        self.remaining = remaining;
        Ok(byte)
    }

    fn short_string(&mut self, maximum: usize) -> Result<String, RoomRouteCodeError> {
        let length = usize::from(self.byte()?);
        if length == 0 || length > maximum || self.remaining.len() < length {
            return Err(RoomRouteCodeError::InvalidFormat);
        }
        let (bytes, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| RoomRouteCodeError::InvalidFormat)
    }

    fn finish(self) -> Result<(), RoomRouteCodeError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(RoomRouteCodeError::InvalidFormat)
        }
    }
}

fn push_short(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), RoomRouteCodeError> {
    let length = u8::try_from(bytes.len()).map_err(|_| RoomRouteCodeError::InvalidLocator)?;
    output.push(length);
    output.extend_from_slice(bytes);
    Ok(())
}

fn validate_handle(value: &str) -> Result<(), RoomRouteCodeError> {
    if value.is_empty()
        || value.len() > MAX_ROOM_HANDLE_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RoomRouteCodeError::InvalidLocator);
    }
    Ok(())
}

fn validate_origin(value: &str, require_https: bool) -> Result<(), RoomRouteCodeError> {
    if value.is_empty()
        || value.len() > MAX_ORIGIN_BYTES
        || !value.is_ascii()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(RoomRouteCodeError::InvalidLocator);
    }
    let authority = if require_https {
        value.strip_prefix("https://")
    } else {
        value.strip_prefix("http://")
    }
    .ok_or(RoomRouteCodeError::InvalidLocator)?;
    if authority.is_empty()
        || authority
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
        || !authority.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
    {
        return Err(RoomRouteCodeError::InvalidLocator);
    }
    if !require_https && !is_loopback_authority(authority) {
        return Err(RoomRouteCodeError::InvalidLocator);
    }
    Ok(())
}

fn is_loopback_authority(value: &str) -> bool {
    ["localhost", "127.0.0.1", "[::1]"].iter().any(|host| {
        value == *host
            || value.strip_prefix(host).is_some_and(|suffix| {
                suffix.strip_prefix(':').is_some_and(|port| {
                    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
                })
            })
    })
}

fn checksum(payload: &[u8]) -> [u8; CHECKSUM_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-room-route-code-v1\0");
    hasher.update(payload);
    hasher.finalize().as_bytes()[..CHECKSUM_BYTES]
        .try_into()
        .expect("checksum slice has fixed length")
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

fn unsafe_string_zero(value: &mut String) {
    // `String::as_mut_vec` would require unsafe code, which the workspace
    // forbids. Replacement drops the original allocation; the invitation
    // secret itself is held in a fixed buffer and is explicitly zeroed.
    value.clear();
    value.shrink_to_fit();
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD_KEY: &str = "VLD0:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    fn principal(byte: u8) -> PrincipalId {
        PrincipalId::new(format!("{byte:02x}").repeat(32)).unwrap()
    }

    fn code(route: RoomRouteHint) -> RoomRouteCode {
        RoomRouteCode::from_secret(route, principal(0x2a), 20_000, 10_000, [7; 32]).unwrap()
    }

    #[test]
    fn room_code_direct_gateway_and_explicit_loopback_vectors_round_trip() {
        let routes = [
            RoomRouteHint::direct_veilid(RoomNetwork::VeilidPublic, RECORD_KEY).unwrap(),
            RoomRouteHint::gateway_https("https://poche.example:8443", "room_alpha").unwrap(),
            RoomRouteHint::explicit_loopback_http("http://127.0.0.1:8080", "room-alpha").unwrap(),
        ];
        let mut corpus = Vec::new();
        for route in routes {
            let expected_kind = route.kind();
            let original = code(route);
            let text = original.encode().unwrap();
            assert!(text.expose().len() <= 256);
            assert!(text.expose().starts_with("p3r-"));
            assert_eq!(format!("{original:?}"), "RoomRouteCode(<redacted>)");
            assert_eq!(format!("{text:?}"), "RoomRouteCodeText(<redacted>)");
            let decoded = RoomRouteCode::decode(text.expose(), 10_000).unwrap();
            assert_eq!(decoded.route_kind(), expected_kind);
            assert_eq!(decoded.host_principal(), &principal(0x2a));
            assert_eq!(decoded.expires_at_unix_ms(), 20_000);
            assert_eq!(decoded.encode().unwrap().expose(), text.expose());
            corpus.extend_from_slice(&u16::try_from(text.expose().len()).unwrap().to_be_bytes());
            corpus.extend_from_slice(text.expose().as_bytes());
        }
        assert_eq!(
            hex(blake3::hash(&corpus).as_bytes()),
            "b06e4dd25ce49e12663f02054a03b8e802f7dfbf36cd44985de32127b671560a"
        );
    }

    #[test]
    fn room_code_old_prefix_keeps_its_v1_meaning() {
        let old = crate::RoomCode::issue(
            RoomNetwork::VeilidLocal,
            RECORD_KEY,
            principal(0x11),
            20_000,
            10_000,
        )
        .unwrap();
        let old_text = old.encode().unwrap();
        assert!(old_text.expose().starts_with("p3-"));
        assert_eq!(
            RoomRouteCode::decode(old_text.expose(), 10_000).unwrap_err(),
            RoomRouteCodeError::InvalidFormat
        );
        assert!(crate::RoomCode::decode(old_text.expose(), 10_000).is_ok());
    }

    #[test]
    fn room_code_routes_reject_remote_http_paths_mutation_and_expiry() {
        assert!(RoomRouteHint::gateway_https("http://poche.example", "room").is_err());
        assert!(RoomRouteHint::gateway_https("https://poche.example/path", "room").is_err());
        assert!(RoomRouteHint::explicit_loopback_http("http://192.0.2.1:8080", "room").is_err());
        assert!(RoomRouteHint::explicit_loopback_http("https://localhost:8080", "room").is_err());

        let value = code(RoomRouteHint::gateway_https("https://poche.example", "room").unwrap());
        let text = value.encode().unwrap();
        assert_eq!(
            RoomRouteCode::decode(text.expose(), 20_000).unwrap_err(),
            RoomRouteCodeError::Expired
        );
        let mut mutated = text.expose().as_bytes().to_vec();
        let last = mutated.len() - 1;
        mutated[last] = if mutated[last] == b'A' { b'B' } else { b'A' };
        assert!(matches!(
            RoomRouteCode::decode(std::str::from_utf8(&mutated).unwrap(), 10_000),
            Err(RoomRouteCodeError::ChecksumMismatch | RoomRouteCodeError::InvalidFormat)
        ));
        mutated.fill(0);
    }
}

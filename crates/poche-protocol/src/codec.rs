use core::fmt;

use serde::Serialize;

use crate::{
    CommandEnvelope, EnvelopeValidationError, EventEnvelope, ProtocolFrame, SemanticHash,
    SignatureAlgorithm, SignatureIntent, SnapshotEnvelope, UnsignedCommandEnvelope,
    UnsignedEventEnvelope, UnsignedSnapshotEnvelope,
};

/// Safe application payload ceiling below Veilid's 32,768-byte operation bound.
pub const MAX_FRAME_BYTES: usize = 30_000;

const COMMAND_SIGNING_DOMAIN: &[u8] = b"POCHE\0COMMAND\0V1";
const EVENT_SIGNING_DOMAIN: &[u8] = b"POCHE\0EVENT\0V1";
const SNAPSHOT_SIGNING_DOMAIN: &[u8] = b"POCHE\0SNAPSHOT\0V1";

/// Stable descriptor hashed as the v1 reflected/wire schema identity.
///
/// Facet supplies runtime reflection for every named shape. This descriptor
/// fixes the field order and payload tag vocabulary used by canonical signing
/// and NDJSON so a dependency's debug representation cannot become the schema.
pub const PROTOCOL_SCHEMA_DESCRIPTOR: &str = concat!(
    "poche.protocol.v1\n",
    "frame=command|event|snapshot|projection|error\n",
    "command=protocol_version,room_id,session_epoch,command_id,principal_id,expected_revision,correlation_id,causation_id,payload,signature\n",
    "event=protocol_version,room_id,session_epoch,event_id,principal_id,current_revision,correlation_id,causation_id,payload,signature\n",
    "snapshot=protocol_version,room_id,session_epoch,snapshot_id,principal_id,current_revision,correlation_id,causation_id,payload,signature\n",
    "projection=protocol_version,room_id,session_epoch,projection_id,principal_id,current_revision,projection_epoch,correlation_id,causation_id,payload,signature\n",
    "error=protocol_version,room_id,session_epoch,event_id,principal_id,current_revision,correlation_id,causation_id,payload,signature\n",
    "command-tags=create_room|redeem_invite|take_seat|release_seat|ready|unready|arm_countdown|abort_countdown|countdown_expired|pause|unpause|game_action|apply_chance|settle|chat|request_hand|grant_hand|deny_hand|revoke_hand|reconnect|leave|remove_member|reset_lobby|close_room\n",
    "event-tags=room_created|member_joined|member_disconnected|member_reconnected|member_left|seat_taken|seat_released|ready_changed|countdown_armed|countdown_aborted|phase_changed|game_transitioned|round_scored|chat_posted|hand_requested|hand_granted|hand_denied|hand_revoked|hand_capabilities_expired|room_closed\n",
    "signature=domain_version,algorithm,key_id,signature\n",
    "projection-payload=phase,members,public_game_state,own_hand,granted_hands,public_history\n",
    "public-game-state=schema_version,phase,dealer,actor,round_index,hand_size,hand_counts,trump,current_trick,bids,tricks_won,scores,pot_cents\n",
    "public-game-event-tags=game_started|player_action|round_scored\n",
    "played-card=seat,card\n",
    "refinements=identifier:1..64-canonical-ascii;invite-proof:1..256-utf8-redacted;signature:128-lower-hex;chat:1..2048-utf8;card-code:0..52\n",
    "signature-domain-v1=length-framed-binary;diagnostic-json-is-not-signed-as-is\n",
);

/// Strict frame-codec error containing no rejected input bytes or secret material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    Oversize,
    MissingLineTerminator,
    AmbiguousControlInput,
    InvalidUtf8,
    InvalidJson,
    NonCanonical,
    WrongFrameKind,
    InvalidEnvelope(EnvelopeValidationError),
    Serialization,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Oversize => "protocol frame exceeds the configured bound",
            Self::MissingLineTerminator => "protocol frame requires one LF terminator",
            Self::AmbiguousControlInput => "protocol frame contains an extra control delimiter",
            Self::InvalidUtf8 => "protocol frame is not valid UTF-8",
            Self::InvalidJson => "protocol frame is not valid registered JSON",
            Self::NonCanonical => "protocol frame is not canonical NDJSON",
            Self::WrongFrameKind => "protocol frame has the wrong semantic kind",
            Self::InvalidEnvelope(_) => "protocol envelope failed semantic validation",
            Self::Serialization => "registered protocol shape could not be serialized",
        })
    }
}

impl std::error::Error for CodecError {}

/// Return the stable v1 schema identity.
#[must_use]
pub fn protocol_schema_hash() -> SemanticHash {
    SemanticHash(*blake3::hash(PROTOCOL_SCHEMA_DESCRIPTOR.as_bytes()).as_bytes())
}

/// Encode exactly one canonical NDJSON frame with one trailing LF.
///
/// # Errors
///
/// Returns a structured error for an invalid envelope, serialization failure,
/// or a frame larger than [`MAX_FRAME_BYTES`].
pub fn encode_frame_line(frame: &ProtocolFrame) -> Result<Vec<u8>, CodecError> {
    frame.validate().map_err(CodecError::InvalidEnvelope)?;
    let mut encoded = serde_json::to_vec(frame).map_err(|_| CodecError::Serialization)?;
    if encoded.len() + 1 > MAX_FRAME_BYTES {
        return Err(CodecError::Oversize);
    }
    encoded.push(b'\n');
    Ok(encoded)
}

/// Decode exactly one canonical NDJSON frame.
///
/// Decoding fails closed on CRLF, interior LF/CR, leading/trailing whitespace,
/// reordered/unknown fields, unknown tags or versions, malformed payloads, and
/// extra frames.
///
/// # Errors
///
/// Returns a redacted [`CodecError`] and never includes rejected bytes.
pub fn decode_frame_line(bytes: &[u8]) -> Result<ProtocolFrame, CodecError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(CodecError::Oversize);
    }
    let Some(body) = bytes.strip_suffix(b"\n") else {
        return Err(CodecError::MissingLineTerminator);
    };
    if body.is_empty() || body.iter().any(|byte| matches!(byte, b'\n' | b'\r')) {
        return Err(CodecError::AmbiguousControlInput);
    }
    core::str::from_utf8(body).map_err(|_| CodecError::InvalidUtf8)?;
    let frame: ProtocolFrame = serde_json::from_slice(body).map_err(|_| CodecError::InvalidJson)?;
    frame.validate().map_err(CodecError::InvalidEnvelope)?;
    let canonical = serde_json::to_vec(&frame).map_err(|_| CodecError::Serialization)?;
    if canonical != body {
        return Err(CodecError::NonCanonical);
    }
    Ok(frame)
}

/// Encode one command frame.
///
/// # Errors
///
/// Returns the same errors as [`encode_frame_line`].
pub fn encode_command_line(command: &CommandEnvelope) -> Result<Vec<u8>, CodecError> {
    encode_frame_line(&ProtocolFrame::Command(command.clone()))
}

/// Decode one command frame, rejecting every other registered frame kind.
///
/// # Errors
///
/// Returns the same errors as [`decode_frame_line`] plus
/// [`CodecError::WrongFrameKind`].
pub fn decode_command_line(bytes: &[u8]) -> Result<CommandEnvelope, CodecError> {
    match decode_frame_line(bytes)? {
        ProtocolFrame::Command(command) => Ok(command),
        _ => Err(CodecError::WrongFrameKind),
    }
}

/// Produce versioned length-framed canonical bytes for an unsigned command.
///
/// Diagnostic JSON is deliberately not returned or signed as-is.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_command_signed_bytes(
    command: &UnsignedCommandEnvelope,
) -> Result<Vec<u8>, CodecError> {
    command.validate().map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        COMMAND_SIGNING_DOMAIN,
        command.protocol_version,
        command.room_id.as_str(),
        command.session_epoch,
        command.command_id.as_str(),
        command.principal_id.as_str(),
        command.expected_revision,
        command.correlation_id.as_str(),
        command.causation_id.as_ref().map(crate::EventId::as_str),
        &command.payload,
        &command.signature_intent,
    )
}

/// Reconstruct canonical signed bytes from a signed command for verification.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_command_verification_bytes(
    command: &CommandEnvelope,
) -> Result<Vec<u8>, CodecError> {
    command.validate().map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        COMMAND_SIGNING_DOMAIN,
        command.protocol_version,
        command.room_id.as_str(),
        command.session_epoch,
        command.command_id.as_str(),
        command.principal_id.as_str(),
        command.expected_revision,
        command.correlation_id.as_str(),
        command.causation_id.as_ref().map(crate::EventId::as_str),
        &command.payload,
        &command.signature.intent(),
    )
}

/// Produce versioned length-framed canonical bytes for an unsigned event.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_event_signed_bytes(event: &UnsignedEventEnvelope) -> Result<Vec<u8>, CodecError> {
    event.validate().map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        EVENT_SIGNING_DOMAIN,
        event.protocol_version,
        event.room_id.as_str(),
        event.session_epoch,
        event.event_id.as_str(),
        event.principal_id.as_str(),
        event.current_revision,
        event.correlation_id.as_str(),
        Some(event.causation_id.as_str()),
        &event.payload,
        &event.signature_intent,
    )
}

/// Reconstruct canonical event bytes for signature verification.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_event_verification_bytes(event: &EventEnvelope) -> Result<Vec<u8>, CodecError> {
    let frame = ProtocolFrame::Event(event.clone());
    frame.validate().map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        EVENT_SIGNING_DOMAIN,
        event.protocol_version,
        event.room_id.as_str(),
        event.session_epoch,
        event.event_id.as_str(),
        event.principal_id.as_str(),
        event.current_revision,
        event.correlation_id.as_str(),
        Some(event.causation_id.as_str()),
        &event.payload,
        &event.signature.intent(),
    )
}

/// Produce versioned length-framed canonical bytes for an unsigned snapshot.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_snapshot_signed_bytes(
    snapshot: &UnsignedSnapshotEnvelope,
) -> Result<Vec<u8>, CodecError> {
    snapshot.validate().map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        SNAPSHOT_SIGNING_DOMAIN,
        snapshot.protocol_version,
        snapshot.room_id.as_str(),
        snapshot.session_epoch,
        snapshot.snapshot_id.as_str(),
        snapshot.principal_id.as_str(),
        snapshot.current_revision,
        snapshot.correlation_id.as_str(),
        Some(snapshot.causation_id.as_str()),
        &snapshot.payload,
        &snapshot.signature_intent,
    )
}

/// Reconstruct canonical snapshot bytes for strict signature verification.
///
/// # Errors
///
/// Returns an error for invalid fields or an unencodable registered payload.
pub fn canonical_snapshot_verification_bytes(
    snapshot: &SnapshotEnvelope,
) -> Result<Vec<u8>, CodecError> {
    ProtocolFrame::Snapshot(snapshot.clone())
        .validate()
        .map_err(CodecError::InvalidEnvelope)?;
    signing_bytes(
        SNAPSHOT_SIGNING_DOMAIN,
        snapshot.protocol_version,
        snapshot.room_id.as_str(),
        snapshot.session_epoch,
        snapshot.snapshot_id.as_str(),
        snapshot.principal_id.as_str(),
        snapshot.current_revision,
        snapshot.correlation_id.as_str(),
        Some(snapshot.causation_id.as_str()),
        &snapshot.payload,
        &snapshot.signature.intent(),
    )
}

/// Semantic identity of one command independent of its signature bytes.
///
/// # Errors
///
/// Returns an error when canonical signed bytes cannot be produced.
pub fn command_semantic_hash(
    command: &UnsignedCommandEnvelope,
) -> Result<SemanticHash, CodecError> {
    Ok(SemanticHash(
        *blake3::hash(&canonical_command_signed_bytes(command)?).as_bytes(),
    ))
}

/// Semantic identity reconstructed from one validated signed command.
///
/// Signature bytes themselves are excluded; their intent and every semantic
/// command field remain bound.
///
/// # Errors
///
/// Returns an error when canonical verification bytes cannot be produced.
pub fn verified_command_semantic_hash(
    command: &CommandEnvelope,
) -> Result<SemanticHash, CodecError> {
    Ok(SemanticHash(
        *blake3::hash(&canonical_command_verification_bytes(command)?).as_bytes(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn signing_bytes<T: Serialize>(
    domain: &[u8],
    protocol_version: u16,
    room_id: &str,
    session_epoch: u64,
    object_id: &str,
    principal_id: &str,
    revision: u64,
    correlation_id: &str,
    causation_id: Option<&str>,
    payload: &T,
    intent: &SignatureIntent,
) -> Result<Vec<u8>, CodecError> {
    let payload = serde_json::to_vec(payload).map_err(|_| CodecError::Serialization)?;
    let mut writer = CanonicalWriter::new(domain);
    writer.u16(protocol_version);
    writer.text(room_id)?;
    writer.u64(session_epoch);
    writer.text(object_id)?;
    writer.text(principal_id)?;
    writer.u64(revision);
    writer.text(correlation_id)?;
    writer.optional_text(causation_id)?;
    writer.bytes(&payload)?;
    writer.u16(intent.domain_version);
    writer.u8(match intent.algorithm {
        SignatureAlgorithm::Ed25519 => 1,
    });
    writer.text(intent.key_id.as_str())?;
    Ok(writer.finish())
}

struct CanonicalWriter {
    bytes: Vec<u8>,
}

impl CanonicalWriter {
    fn new(domain: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(domain);
        Self { bytes }
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn text(&mut self, value: &str) -> Result<(), CodecError> {
        self.bytes(value.as_bytes())
    }

    fn optional_text(&mut self, value: Option<&str>) -> Result<(), CodecError> {
        if let Some(value) = value {
            self.u8(1);
            self.text(value)
        } else {
            self.u8(0);
            Ok(())
        }
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), CodecError> {
        let length = u32::try_from(value.len()).map_err(|_| CodecError::Oversize)?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CommandId, CommandPayload, CorrelationId, PROTOCOL_VERSION_V1, PrincipalId, RoomId,
        SIGNATURE_DOMAIN_V1, SignatureBytes,
    };

    fn unsigned_chat(text: &str) -> UnsignedCommandEnvelope {
        let principal_id = PrincipalId::new("principal-alice").unwrap();
        UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("room-demo").unwrap(),
            session_epoch: 3,
            command_id: CommandId::new("command-0001").unwrap(),
            principal_id: principal_id.clone(),
            expected_revision: 7,
            correlation_id: CorrelationId::new("correlation-0001").unwrap(),
            causation_id: None,
            payload: CommandPayload::Chat {
                text: text.to_owned(),
            },
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: principal_id,
            },
        }
    }

    fn signed_chat(text: &str) -> CommandEnvelope {
        unsigned_chat(text).attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
    }

    #[test]
    fn canonical_command_and_verification_bytes_match() {
        let unsigned = unsigned_chat("hello");
        let signed = unsigned
            .clone()
            .attach_signature(SignatureBytes::new("a".repeat(128)).unwrap());
        assert_eq!(
            canonical_command_signed_bytes(&unsigned).unwrap(),
            canonical_command_verification_bytes(&signed).unwrap()
        );
        assert!(
            !canonical_command_signed_bytes(&unsigned)
                .unwrap()
                .starts_with(b"{")
        );
        assert_eq!(
            lower_hex(&canonical_command_signed_bytes(&unsigned).unwrap()),
            "504f43484500434f4d4d414e44005631000100000009726f6f6d2d64656d6f00000000000000030000000c636f6d6d616e642d303030310000000f7072696e636970616c2d616c696365000000000000000700000010636f7272656c6174696f6e2d3030303100000000277b226b696e64223a2263686174222c2264617461223a7b2274657874223a2268656c6c6f227d7d0001010000000f7072696e636970616c2d616c696365"
        );
        assert_eq!(
            encode_command_line(&signed_chat("hello")).unwrap(),
            include_bytes!("../../../fixtures/protocol/command-chat-v1.ndjson")
        );
    }

    #[test]
    fn chat_newline_is_payload_data_not_a_second_frame() {
        let line = encode_command_line(&signed_chat("first\nsecond")).unwrap();
        assert_eq!(line.last(), Some(&b'\n'));
        assert!(!line[..line.len() - 1].contains(&b'\n'));
        assert!(line.windows(2).any(|window| window == b"\\n"));
        assert_eq!(
            decode_command_line(&line).unwrap(),
            signed_chat("first\nsecond")
        );
    }

    #[test]
    fn decoder_rejects_ambiguous_and_noncanonical_control_input() {
        let canonical = encode_command_line(&signed_chat("hello")).unwrap();
        let mut doubled = canonical.clone();
        doubled.extend_from_slice(&canonical);
        assert_eq!(
            decode_frame_line(&doubled),
            Err(CodecError::AmbiguousControlInput)
        );

        let mut crlf = canonical;
        crlf.insert(crlf.len() - 1, b'\r');
        assert_eq!(
            decode_frame_line(&crlf),
            Err(CodecError::AmbiguousControlInput)
        );
    }

    #[test]
    fn schema_hash_is_reproducible() {
        assert_eq!(
            protocol_schema_hash(),
            SemanticHash([
                0x14, 0x89, 0xb2, 0x88, 0x7a, 0xcc, 0x11, 0x7d, 0xd1, 0xa9, 0xe9, 0x8d, 0x28, 0x91,
                0xb8, 0x64, 0x0f, 0xa4, 0xd9, 0x01, 0x62, 0x1b, 0x73, 0x4d, 0x1f, 0x76, 0x54, 0x94,
                0x0c, 0x4e, 0x11, 0xad,
            ])
        );
        assert_eq!(protocol_schema_hash(), protocol_schema_hash());
        assert_ne!(protocol_schema_hash(), SemanticHash([0; 32]));
        assert_eq!(
            facet::shape_of::<ProtocolFrame>().type_identifier,
            "ProtocolFrame"
        );
        assert_eq!(
            facet::shape_of::<CommandEnvelope>().type_identifier,
            "CommandEnvelope"
        );
        assert_eq!(
            facet::shape_of::<EventEnvelope>().type_identifier,
            "EventEnvelope"
        );
        assert_eq!(
            facet::shape_of::<crate::SnapshotEnvelope>().type_identifier,
            "SnapshotEnvelope"
        );
        assert_eq!(
            facet::shape_of::<crate::ProjectionEnvelope>().type_identifier,
            "ProjectionEnvelope"
        );
        assert_eq!(
            facet::shape_of::<crate::ErrorEnvelope>().type_identifier,
            "ErrorEnvelope"
        );
    }

    fn lower_hex(bytes: &[u8]) -> String {
        use core::fmt::Write as _;

        let mut result = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut result, "{byte:02x}").unwrap();
        }
        result
    }
}

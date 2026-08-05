// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_protocol::{
    CommandEnvelope, CommandId, DenyReason, ProtocolFrame, SemanticHash, encode_command_line,
    encode_frame_line, verified_command_semantic_hash,
};
use serde::{Deserialize, Serialize};

use crate::{ApplicationPublicIdentity, EncryptedProjectionPacket, verify_command_signature};

const TRANSPORT_SCHEMA_VERSION: u16 = 2;
const MAX_TRANSPORT_CALL_BYTES: usize = 30_000;

/// Stable transport failures used by retry policy without leaking Veilid
/// diagnostics or route material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFailure {
    TryAgain,
    Timeout,
    NoConnection,
    StaleRoute,
    WatchRenewal,
    Shutdown,
    Oversized,
    InvalidMessage,
    Permanent,
}

/// Required next step after one failed, idempotent command call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAction {
    RetrySameRoute,
    RefreshRendezvousThenRetry,
    StopShutdown,
    StopPermanent,
    Exhausted,
}

/// Strict transport-wire failures containing no rejected bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportWireError {
    Oversized,
    InvalidEncoding,
    NonCanonical,
    InvalidCommand,
    InvalidReply,
    RetryConflict,
    RetryExhausted,
}

/// One stable-key-signed command carried by a Veilid `AppCall`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportCommandCall {
    pub schema_version: u16,
    pub identity: ApplicationPublicIdentity,
    pub command: CommandEnvelope,
}

impl TransportCommandCall {
    /// Construct and validate one command call.
    ///
    /// # Errors
    ///
    /// Rejects malformed command envelopes before network use.
    pub fn new(
        identity: ApplicationPublicIdentity,
        command: CommandEnvelope,
    ) -> Result<Self, TransportWireError> {
        verified_command_semantic_hash(&command).map_err(|_| TransportWireError::InvalidCommand)?;
        verify_command_signature(&command, &identity)
            .map_err(|_| TransportWireError::InvalidCommand)?;
        Ok(Self {
            schema_version: TRANSPORT_SCHEMA_VERSION,
            identity,
            command,
        })
    }

    /// Encode strict canonical JSON below Poche's safe `AppCall` ceiling.
    ///
    /// # Errors
    ///
    /// Rejects invalid calls, serialization failures, and oversized output.
    pub fn encode(&self) -> Result<Vec<u8>, TransportWireError> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|_| TransportWireError::InvalidEncoding)?;
        if encoded.len() > MAX_TRANSPORT_CALL_BYTES {
            return Err(TransportWireError::Oversized);
        }
        Ok(encoded)
    }

    /// Decode strict canonical JSON without retaining rejected input.
    ///
    /// # Errors
    ///
    /// Rejects unknown fields, noncanonical input, invalid commands, and size
    /// violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, TransportWireError> {
        if bytes.len() > MAX_TRANSPORT_CALL_BYTES {
            return Err(TransportWireError::Oversized);
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| TransportWireError::InvalidEncoding)?;
        value.validate()?;
        let canonical =
            serde_json::to_vec(&value).map_err(|_| TransportWireError::InvalidEncoding)?;
        if canonical != bytes {
            return Err(TransportWireError::NonCanonical);
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), TransportWireError> {
        if self.schema_version != TRANSPORT_SCHEMA_VERSION {
            return Err(TransportWireError::InvalidCommand);
        }
        verified_command_semantic_hash(&self.command)
            .map_err(|_| TransportWireError::InvalidCommand)?;
        verify_command_signature(&self.command, &self.identity)
            .map_err(|_| TransportWireError::InvalidCommand)
    }
}

/// Authority result carried by a command reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportDisposition {
    Applied,
    Duplicate,
    Denied,
    RecoveryRequired,
}

/// One authority reply. Events retain authority revision order, public errors
/// remain protocol frames, and viewer-private state is opaque HPKE ciphertext.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportCommandReply {
    pub schema_version: u16,
    pub command_id: CommandId,
    pub disposition: TransportDisposition,
    pub denial: Option<DenyReason>,
    pub base_revision: u64,
    pub current_revision: u64,
    pub frames: Vec<ProtocolFrame>,
    pub encrypted_projection: Option<EncryptedProjectionPacket>,
}

impl TransportCommandReply {
    /// Build a reply and validate its revision/frame contract.
    ///
    /// # Errors
    ///
    /// Rejects inconsistent disposition, denial, frame, or revision shapes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        disposition: TransportDisposition,
        denial: Option<DenyReason>,
        base_revision: u64,
        current_revision: u64,
        frames: Vec<ProtocolFrame>,
        encrypted_projection: Option<EncryptedProjectionPacket>,
    ) -> Result<Self, TransportWireError> {
        let value = Self {
            schema_version: TRANSPORT_SCHEMA_VERSION,
            command_id,
            disposition,
            denial,
            base_revision,
            current_revision,
            frames,
            encrypted_projection,
        };
        value.validate()?;
        Ok(value)
    }

    /// Encode strict canonical JSON below Poche's safe `AppCall` ceiling.
    ///
    /// # Errors
    ///
    /// Rejects invalid replies, serialization failures, and oversized output.
    pub fn encode(&self) -> Result<Vec<u8>, TransportWireError> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|_| TransportWireError::InvalidEncoding)?;
        if encoded.len() > MAX_TRANSPORT_CALL_BYTES {
            return Err(TransportWireError::Oversized);
        }
        Ok(encoded)
    }

    /// Decode and re-encode strict canonical reply JSON.
    ///
    /// # Errors
    ///
    /// Rejects unknown fields, noncanonical input, invalid frames/revisions,
    /// and size violations.
    pub fn decode(bytes: &[u8]) -> Result<Self, TransportWireError> {
        if bytes.len() > MAX_TRANSPORT_CALL_BYTES {
            return Err(TransportWireError::Oversized);
        }
        let value: Self =
            serde_json::from_slice(bytes).map_err(|_| TransportWireError::InvalidEncoding)?;
        value.validate()?;
        let canonical =
            serde_json::to_vec(&value).map_err(|_| TransportWireError::InvalidEncoding)?;
        if canonical != bytes {
            return Err(TransportWireError::NonCanonical);
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), TransportWireError> {
        if self.schema_version != TRANSPORT_SCHEMA_VERSION
            || self.current_revision < self.base_revision
            || matches!(self.disposition, TransportDisposition::Denied) != self.denial.is_some()
            || (!matches!(self.disposition, TransportDisposition::Denied) && self.denial.is_some())
        {
            return Err(TransportWireError::InvalidReply);
        }
        let mut expected_event_revision = self.base_revision;
        let mut saw_non_event = false;
        for frame in &self.frames {
            encode_frame_line(frame).map_err(|_| TransportWireError::InvalidReply)?;
            match frame {
                ProtocolFrame::Event(event) if !saw_non_event => {
                    expected_event_revision = expected_event_revision
                        .checked_add(1)
                        .ok_or(TransportWireError::InvalidReply)?;
                    if event.current_revision != expected_event_revision {
                        return Err(TransportWireError::InvalidReply);
                    }
                }
                ProtocolFrame::Error(error) => {
                    saw_non_event = true;
                    if error.current_revision != self.current_revision {
                        return Err(TransportWireError::InvalidReply);
                    }
                }
                _ => return Err(TransportWireError::InvalidReply),
            }
        }
        if expected_event_revision != self.current_revision
            && matches!(
                self.disposition,
                TransportDisposition::Applied | TransportDisposition::Duplicate
            )
        {
            return Err(TransportWireError::InvalidReply);
        }
        if matches!(self.disposition, TransportDisposition::Denied)
            && (self.current_revision != self.base_revision
                || self.encrypted_projection.is_some()
                || self
                    .frames
                    .iter()
                    .any(|frame| !matches!(frame, ProtocolFrame::Error(_))))
        {
            return Err(TransportWireError::InvalidReply);
        }
        if let Some(projection) = &self.encrypted_projection {
            projection
                .validate()
                .map_err(|_| TransportWireError::InvalidReply)?;
            if projection.current_revision != self.current_revision {
                return Err(TransportWireError::InvalidReply);
            }
        }
        Ok(())
    }
}

/// Retry state bound to the exact semantic command content and command ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRetryState {
    command_id: CommandId,
    command_hash: SemanticHash,
    attempts: u8,
    max_attempts: u8,
}

impl CommandRetryState {
    /// Start an idempotent retry budget for one exact command.
    ///
    /// # Errors
    ///
    /// Rejects invalid commands or a zero retry budget.
    pub fn new(command: &CommandEnvelope, max_attempts: u8) -> Result<Self, TransportWireError> {
        if max_attempts == 0 {
            return Err(TransportWireError::RetryExhausted);
        }
        let command_hash = retry_command_hash(command)?;
        Ok(Self {
            command_id: command.command_id.clone(),
            command_hash,
            attempts: 0,
            max_attempts,
        })
    }

    /// Record an attempt while proving that a caller did not change the
    /// command ID or signed semantic content between retries.
    ///
    /// # Errors
    ///
    /// Rejects conflicting content or an exhausted budget.
    pub fn begin_attempt(&mut self, command: &CommandEnvelope) -> Result<u8, TransportWireError> {
        let hash = retry_command_hash(command)?;
        if command.command_id != self.command_id || hash != self.command_hash {
            return Err(TransportWireError::RetryConflict);
        }
        if self.attempts >= self.max_attempts {
            return Err(TransportWireError::RetryExhausted);
        }
        self.attempts += 1;
        Ok(self.attempts)
    }

    /// Classify the next safe action after the most recent failed attempt.
    #[must_use]
    pub const fn after_failure(&self, failure: TransportFailure) -> RetryAction {
        if self.attempts >= self.max_attempts {
            return RetryAction::Exhausted;
        }
        match failure {
            TransportFailure::TryAgain | TransportFailure::Timeout => RetryAction::RetrySameRoute,
            TransportFailure::NoConnection
            | TransportFailure::StaleRoute
            | TransportFailure::WatchRenewal => RetryAction::RefreshRendezvousThenRetry,
            TransportFailure::Shutdown => RetryAction::StopShutdown,
            TransportFailure::Oversized
            | TransportFailure::InvalidMessage
            | TransportFailure::Permanent => RetryAction::StopPermanent,
        }
    }

    #[must_use]
    pub const fn attempts(&self) -> u8 {
        self.attempts
    }
}

/// Non-authoritative meaning assigned to Veilid update notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousHint {
    Ignore,
    FetchAndValidate,
    RenewWatchThenFetchAndValidate,
    ReplaceRouteFromValidatedRendezvous,
    Shutdown,
}

/// Classify a watched-value callback while explicitly discarding its value and
/// delivery order. A matching live watch always causes a validated fetch.
#[must_use]
pub const fn classify_rendezvous_value_hint(
    matches_current_record: bool,
    watch_alive: bool,
) -> RendezvousHint {
    if !matches_current_record {
        RendezvousHint::Ignore
    } else if watch_alive {
        RendezvousHint::FetchAndValidate
    } else {
        RendezvousHint::RenewWatchThenFetchAndValidate
    }
}

/// Classify a route-death callback without treating it as identity evidence.
#[must_use]
pub const fn classify_rendezvous_route_hint(current_route_died: bool) -> RendezvousHint {
    if current_route_died {
        RendezvousHint::ReplaceRouteFromValidatedRendezvous
    } else {
        RendezvousHint::Ignore
    }
}

/// Read-only client countdown estimate. Reaching zero still waits for the
/// authority's signed phase transition; this type cannot emit commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountdownDisplayEstimate {
    pub remaining_ticks: u64,
    pub awaiting_authority_transition: bool,
}

/// Estimate remaining logical ticks from the last authority clock sample.
/// Saturation represents display uncertainty, never permission to start.
#[must_use]
pub const fn estimate_countdown(
    deadline_tick: u64,
    sampled_authority_tick: u64,
    locally_elapsed_ticks: u64,
) -> CountdownDisplayEstimate {
    let estimated_now = sampled_authority_tick.saturating_add(locally_elapsed_ticks);
    let remaining_ticks = deadline_tick.saturating_sub(estimated_now);
    CountdownDisplayEstimate {
        remaining_ticks,
        awaiting_authority_transition: remaining_ticks == 0,
    }
}

fn retry_command_hash(command: &CommandEnvelope) -> Result<SemanticHash, TransportWireError> {
    let encoded = encode_command_line(command).map_err(|_| TransportWireError::InvalidCommand)?;
    Ok(SemanticHash(*blake3::hash(&encoded).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApplicationIdentity, ExplicitInsecureDevelopment, IdentityStoragePolicy,
        InsecureMemoryIdentityStore,
    };
    use poche_protocol::{
        CommandPayload, CorrelationId, PROTOCOL_VERSION_V1, PrincipalId, RoomId,
        SIGNATURE_DOMAIN_V1, SignatureAlgorithm, SignatureBytes, SignatureIntent,
    };
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn command() -> CommandEnvelope {
        let principal = PrincipalId::new("transport-principal").unwrap();
        poche_protocol::UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("transport-room").unwrap(),
            session_epoch: 1,
            command_id: CommandId::new("transport-command").unwrap(),
            principal_id: principal.clone(),
            expected_revision: 7,
            correlation_id: CorrelationId::new("transport-correlation").unwrap(),
            causation_id: None,
            payload: CommandPayload::Chat {
                text: "retry me exactly".to_owned(),
            },
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: principal,
            },
        }
        .attach_signature(SignatureBytes::new("0".repeat(128)).unwrap())
    }

    fn cryptographic_call() -> TransportCommandCall {
        let store = InsecureMemoryIdentityStore::new(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        );
        let identity = block_on(ApplicationIdentity::load_or_create(
            &store,
            IdentityStoragePolicy::AllowExplicitInsecure(
                ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
            ),
        ))
        .unwrap();
        let public = identity.public();
        let unsigned = poche_protocol::UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("transport-room").unwrap(),
            session_epoch: 1,
            command_id: CommandId::new("transport-command").unwrap(),
            principal_id: public.principal_id.clone(),
            expected_revision: 7,
            correlation_id: CorrelationId::new("transport-correlation").unwrap(),
            causation_id: None,
            payload: CommandPayload::Chat {
                text: "signed transport".to_owned(),
            },
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: public.principal_id.clone(),
            },
        };
        TransportCommandCall::new(public, identity.sign_command(unsigned).unwrap()).unwrap()
    }

    #[test]
    fn command_call_is_strict_canonical_and_bounded() {
        let call = cryptographic_call();
        let encoded = call.encode().unwrap();
        assert_eq!(TransportCommandCall::decode(&encoded).unwrap(), call);

        let mut noncanonical = b" ".to_vec();
        noncanonical.extend_from_slice(&encoded);
        assert!(TransportCommandCall::decode(&noncanonical).is_err());
        let unknown = String::from_utf8(encoded).unwrap().replace(
            "{\"schema_version\":2,",
            "{\"unknown\":0,\"schema_version\":2,",
        );
        assert_eq!(
            TransportCommandCall::decode(unknown.as_bytes()),
            Err(TransportWireError::InvalidEncoding)
        );
    }

    #[test]
    fn retry_preserves_exact_command_and_exhausts_deterministically() {
        let command = command();
        let mut retry = CommandRetryState::new(&command, 3).unwrap();
        assert_eq!(retry.begin_attempt(&command), Ok(1));
        assert_eq!(
            retry.after_failure(TransportFailure::TryAgain),
            RetryAction::RetrySameRoute
        );
        assert_eq!(retry.begin_attempt(&command), Ok(2));
        assert_eq!(
            retry.after_failure(TransportFailure::StaleRoute),
            RetryAction::RefreshRendezvousThenRetry
        );

        let mut conflicting = command.clone();
        conflicting.expected_revision += 1;
        assert_eq!(
            retry.begin_attempt(&conflicting),
            Err(TransportWireError::RetryConflict)
        );
        assert_eq!(retry.begin_attempt(&command), Ok(3));
        assert_eq!(
            retry.after_failure(TransportFailure::Timeout),
            RetryAction::Exhausted
        );
        assert_eq!(
            retry.begin_attempt(&command),
            Err(TransportWireError::RetryExhausted)
        );
    }

    #[test]
    fn reply_codec_rejects_revision_claims_without_authority_events() {
        let command_id = CommandId::new("reply-command").unwrap();
        assert_eq!(
            TransportCommandReply::new(
                command_id.clone(),
                TransportDisposition::Applied,
                None,
                7,
                8,
                Vec::new(),
                None,
            ),
            Err(TransportWireError::InvalidReply)
        );
        let denied = TransportCommandReply::new(
            command_id,
            TransportDisposition::Denied,
            Some(DenyReason::WrongPhase),
            7,
            7,
            Vec::new(),
            None,
        )
        .unwrap();
        let encoded = denied.encode().unwrap();
        assert_eq!(TransportCommandReply::decode(&encoded).unwrap(), denied);
    }

    #[test]
    fn every_failure_has_an_explicit_retry_disposition() {
        let mut retry = CommandRetryState::new(&command(), 2).unwrap();
        retry.begin_attempt(&command()).unwrap();
        let cases = [
            (TransportFailure::TryAgain, RetryAction::RetrySameRoute),
            (TransportFailure::Timeout, RetryAction::RetrySameRoute),
            (
                TransportFailure::NoConnection,
                RetryAction::RefreshRendezvousThenRetry,
            ),
            (
                TransportFailure::StaleRoute,
                RetryAction::RefreshRendezvousThenRetry,
            ),
            (
                TransportFailure::WatchRenewal,
                RetryAction::RefreshRendezvousThenRetry,
            ),
            (TransportFailure::Shutdown, RetryAction::StopShutdown),
            (TransportFailure::Oversized, RetryAction::StopPermanent),
            (TransportFailure::InvalidMessage, RetryAction::StopPermanent),
            (TransportFailure::Permanent, RetryAction::StopPermanent),
        ];
        for (failure, expected) in cases {
            assert_eq!(retry.after_failure(failure), expected);
        }
    }

    #[test]
    fn countdown_estimate_never_claims_authority_transition() {
        assert_eq!(
            estimate_countdown(50, 40, 3),
            CountdownDisplayEstimate {
                remaining_ticks: 7,
                awaiting_authority_transition: false,
            }
        );
        assert_eq!(
            estimate_countdown(50, 40, 20),
            CountdownDisplayEstimate {
                remaining_ticks: 0,
                awaiting_authority_transition: true,
            }
        );
    }

    #[test]
    fn watch_and_route_updates_are_only_refresh_hints() {
        assert_eq!(
            classify_rendezvous_value_hint(true, true),
            RendezvousHint::FetchAndValidate
        );
        assert_eq!(
            classify_rendezvous_value_hint(true, false),
            RendezvousHint::RenewWatchThenFetchAndValidate
        );
        assert_eq!(
            classify_rendezvous_value_hint(false, false),
            RendezvousHint::Ignore
        );
        assert_eq!(
            classify_rendezvous_route_hint(true),
            RendezvousHint::ReplaceRouteFromValidatedRendezvous
        );
        assert_eq!(
            classify_rendezvous_route_hint(false),
            RendezvousHint::Ignore
        );
    }
}

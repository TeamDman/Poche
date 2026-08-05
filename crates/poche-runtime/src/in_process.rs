use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;

use poche_protocol::{
    CodecError, CommandEnvelope, CommandId, CommandPayload, CorrelationId, CountdownToken,
    DenyReason, ErrorEnvelope, ErrorPayload, EventId, PROTOCOL_VERSION_V1, PrincipalId,
    ProjectionEnvelope, ProjectionId, ProtocolFrame, RoomId, SIGNATURE_DOMAIN_V1,
    SignatureAlgorithm, SignatureBytes, SignatureIntent, SignatureMetadata,
    UnsignedCommandEnvelope, decode_command_line, decode_frame_line, encode_command_line,
    encode_frame_line,
};
use poche_session::{
    AuthorizedCommand, PolicyDecision, ProjectionError, PureSessionMachine, SessionError,
    SessionEvent, SessionEventKind, SessionGame, SessionMachine, SessionState, apply, authorize,
    decide, decide_transport_disconnect, project_viewer,
};

use crate::{AuthorityTransportPort, ClientPort, ClockPort, TransportPort};

/// Framing mode for an otherwise identical no-socket loopback path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopbackCodec {
    /// Pass registered Rust values directly.
    #[default]
    Typed,
    /// Encode and strictly decode one canonical NDJSON frame at each boundary.
    CanonicalNdjson,
}

/// Deterministic fault applied to the next matching client submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InProcessFault {
    DuplicateNext,
    ReorderNextPair,
    DisconnectNext,
}

/// Stable connection handle; it conveys no semantic capability by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(u64);

/// One connection-bound scripted client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptedClient {
    connection_id: ConnectionId,
    principal_id: PrincipalId,
}

impl ScriptedClient {
    /// Build a structurally signed command for the current in-process state.
    ///
    /// The loopback signature is intentionally a non-cryptographic placeholder;
    /// the connection binding is the Task 4 authentication boundary and Task 5
    /// replaces this helper with stable application keys.
    ///
    /// # Errors
    ///
    /// Returns an identifier/signature construction failure.
    pub fn command<G>(
        &self,
        state: &SessionState<G>,
        command_id: &str,
        payload: CommandPayload,
    ) -> Result<CommandEnvelope, InProcessTransportError> {
        let command_id =
            CommandId::new(command_id).map_err(|_| InProcessTransportError::InvalidIdentifier)?;
        let correlation_id = CorrelationId::new(format!("cor-{}", command_id.as_str()))
            .map_err(|_| InProcessTransportError::InvalidIdentifier)?;
        let signature = loopback_signature_bytes()?;
        Ok(UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: state.room_id.clone(),
            session_epoch: state.session_epoch,
            command_id,
            principal_id: self.principal_id.clone(),
            expected_revision: state.revision,
            correlation_id,
            causation_id: None,
            payload,
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: self.principal_id.clone(),
            },
        }
        .attach_signature(signature))
    }

    #[must_use]
    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
}

impl ClientPort for ScriptedClient {
    type Transport = InProcessTransport;
    type Error = InProcessTransportError;

    fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    fn submit(
        &self,
        transport: &mut Self::Transport,
        command: CommandEnvelope,
    ) -> Result<(), Self::Error> {
        transport.submit(self.connection_id, command)
    }

    fn receive(
        &self,
        transport: &mut Self::Transport,
    ) -> Result<Option<ProtocolFrame>, Self::Error> {
        transport.client_receive(self.connection_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Connection {
    principal_id: PrincipalId,
    connected: bool,
    outbox: VecDeque<ProtocolFrame>,
}

/// An input whose principal was bound by the transport connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticatedIngress {
    Command {
        connection_id: ConnectionId,
        principal_id: PrincipalId,
        command: Box<CommandEnvelope>,
    },
    Disconnected {
        connection_id: ConnectionId,
        principal_id: PrincipalId,
        observation_id: CommandId,
    },
}

/// Failures before the pure session reducer is entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InProcessTransportError {
    UnknownConnection,
    ConnectionClosed,
    PrincipalAlreadyConnected,
    PrincipalBindingMismatch,
    AmbiguousRecipient,
    InvalidIdentifier,
    Codec(CodecError),
    DisconnectedByFault,
}

impl From<CodecError> for InProcessTransportError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

/// Deterministic multi-client transport with no sockets, threads, or wall time.
#[derive(Debug)]
pub struct InProcessTransport {
    codec: LoopbackCodec,
    next_connection: u64,
    next_observation: u64,
    connections: BTreeMap<ConnectionId, Connection>,
    ingress: VecDeque<AuthenticatedIngress>,
    faults: VecDeque<InProcessFault>,
    held_for_reorder: Option<AuthenticatedIngress>,
}

impl InProcessTransport {
    #[must_use]
    pub const fn new(codec: LoopbackCodec) -> Self {
        Self {
            codec,
            next_connection: 0,
            next_observation: 0,
            connections: BTreeMap::new(),
            ingress: VecDeque::new(),
            faults: VecDeque::new(),
            held_for_reorder: None,
        }
    }

    /// Bind a new local connection to one stable application principal.
    ///
    /// # Errors
    ///
    /// Rejects a second simultaneously connected route for the same principal.
    pub fn connect(
        &mut self,
        principal_id: PrincipalId,
    ) -> Result<ScriptedClient, InProcessTransportError> {
        if self
            .connections
            .values()
            .any(|connection| connection.connected && connection.principal_id == principal_id)
        {
            return Err(InProcessTransportError::PrincipalAlreadyConnected);
        }
        let id = ConnectionId(self.next_connection);
        self.next_connection = self.next_connection.saturating_add(1);
        self.connections.insert(
            id,
            Connection {
                principal_id: principal_id.clone(),
                connected: true,
                outbox: VecDeque::new(),
            },
        );
        Ok(ScriptedClient {
            connection_id: id,
            principal_id,
        })
    }

    pub fn inject_fault(&mut self, fault: InProcessFault) {
        self.faults.push_back(fault);
    }

    /// Observe a transport loss and enqueue its semantic boundary event.
    ///
    /// # Errors
    ///
    /// Rejects unknown or already-closed connections.
    pub fn disconnect(
        &mut self,
        connection_id: ConnectionId,
    ) -> Result<(), InProcessTransportError> {
        let connection = self
            .connections
            .get_mut(&connection_id)
            .ok_or(InProcessTransportError::UnknownConnection)?;
        if !connection.connected {
            return Err(InProcessTransportError::ConnectionClosed);
        }
        connection.connected = false;
        let observation_id = CommandId::new(format!("disconnect-{}", self.next_observation))
            .map_err(|_| InProcessTransportError::InvalidIdentifier)?;
        self.next_observation = self.next_observation.saturating_add(1);
        self.ingress.push_back(AuthenticatedIngress::Disconnected {
            connection_id,
            principal_id: connection.principal_id.clone(),
            observation_id,
        });
        Ok(())
    }

    fn submit(
        &mut self,
        connection_id: ConnectionId,
        command: CommandEnvelope,
    ) -> Result<(), InProcessTransportError> {
        let connection = self
            .connections
            .get(&connection_id)
            .ok_or(InProcessTransportError::UnknownConnection)?;
        if !connection.connected {
            return Err(InProcessTransportError::ConnectionClosed);
        }
        if connection.principal_id != command.principal_id {
            return Err(InProcessTransportError::PrincipalBindingMismatch);
        }
        let command = self.round_trip_command(command)?;
        let ingress = AuthenticatedIngress::Command {
            connection_id,
            principal_id: connection.principal_id.clone(),
            command: Box::new(command),
        };
        if let Some(held) = self.held_for_reorder.take() {
            self.ingress.push_back(ingress);
            self.ingress.push_back(held);
            return Ok(());
        }
        match self.faults.pop_front() {
            Some(InProcessFault::DuplicateNext) => {
                self.ingress.push_back(ingress.clone());
                self.ingress.push_back(ingress);
                Ok(())
            }
            Some(InProcessFault::ReorderNextPair) => {
                self.held_for_reorder = Some(ingress);
                Ok(())
            }
            Some(InProcessFault::DisconnectNext) => {
                self.disconnect(connection_id)?;
                Err(InProcessTransportError::DisconnectedByFault)
            }
            None => {
                self.ingress.push_back(ingress);
                Ok(())
            }
        }
    }

    fn round_trip_command(
        &self,
        command: CommandEnvelope,
    ) -> Result<CommandEnvelope, InProcessTransportError> {
        match self.codec {
            LoopbackCodec::Typed => Ok(command),
            LoopbackCodec::CanonicalNdjson => {
                let line = encode_command_line(&command)?;
                Ok(decode_command_line(&line)?)
            }
        }
    }

    fn round_trip_frame(
        &self,
        frame: ProtocolFrame,
    ) -> Result<ProtocolFrame, InProcessTransportError> {
        match self.codec {
            LoopbackCodec::Typed => Ok(frame),
            LoopbackCodec::CanonicalNdjson => {
                let line = encode_frame_line(&frame)?;
                Ok(decode_frame_line(&line)?)
            }
        }
    }

    fn client_receive(
        &mut self,
        connection_id: ConnectionId,
    ) -> Result<Option<ProtocolFrame>, InProcessTransportError> {
        self.connections
            .get_mut(&connection_id)
            .map(|connection| connection.outbox.pop_front())
            .ok_or(InProcessTransportError::UnknownConnection)
    }
}

impl Default for InProcessTransport {
    fn default() -> Self {
        Self::new(LoopbackCodec::Typed)
    }
}

impl TransportPort for InProcessTransport {
    type Error = InProcessTransportError;

    fn send(&mut self, frame: &ProtocolFrame) -> Result<(), Self::Error> {
        let principal = frame_principal(frame);
        let recipients = self
            .connections
            .iter()
            .filter(|(_, connection)| connection.connected && connection.principal_id == *principal)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        let [recipient] = recipients.as_slice() else {
            return Err(InProcessTransportError::AmbiguousRecipient);
        };
        let frame = self.round_trip_frame(frame.clone())?;
        self.connections
            .get_mut(recipient)
            .expect("recipient came from the connection map")
            .outbox
            .push_back(frame);
        Ok(())
    }
}

impl AuthorityTransportPort for InProcessTransport {
    type Ingress = AuthenticatedIngress;

    fn receive(&mut self) -> Option<Self::Ingress> {
        self.ingress.pop_front()
    }
}

fn frame_principal(frame: &ProtocolFrame) -> &PrincipalId {
    match frame {
        ProtocolFrame::Command(envelope) => &envelope.principal_id,
        ProtocolFrame::Event(envelope) => &envelope.principal_id,
        ProtocolFrame::Snapshot(envelope) => &envelope.principal_id,
        ProtocolFrame::Projection(envelope) => &envelope.principal_id,
        ProtocolFrame::Error(envelope) => &envelope.principal_id,
    }
}

/// One scheduled logical-time expiry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledExpiry {
    pub room_id: RoomId,
    pub deadline_tick: u64,
    pub token: CountdownToken,
}

/// Manually advanced clock with deterministic cancellation and due ordering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManualClock {
    now: u64,
    scheduled: Vec<ScheduledExpiry>,
}

impl ManualClock {
    #[must_use]
    pub const fn now(&self) -> u64 {
        self.now
    }

    /// Advance monotonically and return all due expiries in deadline/token order.
    #[must_use]
    pub fn advance_to(&mut self, tick: u64) -> Vec<ScheduledExpiry> {
        self.now = self.now.max(tick);
        self.scheduled.sort_by(|left, right| {
            (left.deadline_tick, left.token.as_str())
                .cmp(&(right.deadline_tick, right.token.as_str()))
        });
        let first_future = self
            .scheduled
            .partition_point(|expiry| expiry.deadline_tick <= self.now);
        self.scheduled.drain(..first_future).collect()
    }
}

impl ClockPort for ManualClock {
    type Error = Infallible;

    fn arm_countdown(
        &mut self,
        room_id: &RoomId,
        deadline_tick: u64,
        token: &CountdownToken,
    ) -> Result<(), Self::Error> {
        self.scheduled
            .retain(|expiry| expiry.room_id != *room_id || expiry.token != *token);
        self.scheduled.push(ScheduledExpiry {
            room_id: room_id.clone(),
            deadline_tick,
            token: token.clone(),
        });
        Ok(())
    }

    fn cancel_countdown(
        &mut self,
        room_id: &RoomId,
        token: &CountdownToken,
    ) -> Result<(), Self::Error> {
        self.scheduled
            .retain(|expiry| expiry.room_id != *room_id || expiry.token != *token);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityDisposition {
    Applied,
    Denied(DenyReason),
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityOutcome {
    pub principal_id: PrincipalId,
    pub command_id: CommandId,
    pub disposition: AuthorityDisposition,
    pub events: usize,
    pub revision: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InProcessAuthorityError<E> {
    Transport(InProcessTransportError),
    Session(SessionError<E>),
    Projection(ProjectionError<E>),
    InvalidIdentifier,
}

/// Central authority composing the existing pure reducer with loopback ports.
pub struct InProcessAuthority<G: SessionGame> {
    pub state: SessionState<G>,
    pub clock: ManualClock,
    pub transport: InProcessTransport,
    next_delivery: u64,
}

impl<G: SessionGame> InProcessAuthority<G> {
    #[must_use]
    pub const fn new(state: SessionState<G>, transport: InProcessTransport) -> Self {
        Self {
            state,
            clock: ManualClock {
                now: 0,
                scheduled: Vec::new(),
            },
            transport,
            next_delivery: 0,
        }
    }

    /// Process at most one authenticated transport input.
    ///
    /// # Errors
    ///
    /// Returns a transport, projection, identifier, invariant, or game error.
    pub fn drive_one(
        &mut self,
    ) -> Result<Option<AuthorityOutcome>, InProcessAuthorityError<G::Error>> {
        let Some(ingress) = self.transport.receive() else {
            return Ok(None);
        };
        match ingress {
            AuthenticatedIngress::Command {
                principal_id,
                command,
                ..
            } => self.process_command(principal_id, *command).map(Some),
            AuthenticatedIngress::Disconnected {
                principal_id,
                observation_id,
                ..
            } => self
                .process_disconnect(principal_id, observation_id)
                .map(Some),
        }
    }

    /// Drain all currently queued input.
    ///
    /// # Errors
    ///
    /// Returns the first runtime boundary error.
    pub fn drive_all(
        &mut self,
    ) -> Result<Vec<AuthorityOutcome>, InProcessAuthorityError<G::Error>> {
        let mut outcomes = Vec::new();
        while let Some(outcome) = self.drive_one()? {
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Advance logical time and deliver due countdown expiries as trusted clock
    /// commands through the same reducer.
    ///
    /// # Errors
    ///
    /// Returns the first runtime boundary error.
    pub fn advance_clock_to(
        &mut self,
        tick: u64,
    ) -> Result<Vec<AuthorityOutcome>, InProcessAuthorityError<G::Error>> {
        let expiries = self.clock.advance_to(tick);
        let mut outcomes = Vec::with_capacity(expiries.len());
        for expiry in expiries {
            let sequence = self.next_delivery;
            self.next_delivery = self.next_delivery.saturating_add(1);
            let principal = self.state.authority_clock.clone();
            let command = runtime_command(
                &self.state,
                &principal,
                &format!("clock-{sequence}"),
                CommandPayload::CountdownExpired {
                    countdown_token: expiry.token,
                },
            )?;
            outcomes.push(self.process_command(principal, command)?);
        }
        Ok(outcomes)
    }

    fn process_command(
        &mut self,
        principal_id: PrincipalId,
        command: CommandEnvelope,
    ) -> Result<AuthorityOutcome, InProcessAuthorityError<G::Error>> {
        let decision = authorize(&self.state, &command);
        if !decision.is_allowed() {
            let reason = decision_reason(&decision).expect("denial has a reason");
            self.send_denial(&command, reason, decision_policy(&decision))?;
            return Ok(AuthorityOutcome {
                principal_id,
                command_id: command.command_id,
                disposition: AuthorityDisposition::Denied(reason),
                events: 0,
                revision: self.state.revision,
            });
        }
        let authorized = AuthorizedCommand::from_decision(command.clone(), decision)
            .expect("an allowed decision should promote");
        let events = match decide(&self.state, &authorized) {
            Ok(events) => events,
            Err(SessionError::Denied(reason)) => {
                self.send_denial(&command, reason, None)?;
                return Ok(AuthorityOutcome {
                    principal_id,
                    command_id: command.command_id,
                    disposition: AuthorityDisposition::Denied(reason),
                    events: 0,
                    revision: self.state.revision,
                });
            }
            Err(error) => return Err(InProcessAuthorityError::Session(error)),
        };
        self.commit_events(&events)?;
        self.broadcast_projections(&command.correlation_id)?;
        Ok(AuthorityOutcome {
            principal_id,
            command_id: command.command_id,
            disposition: AuthorityDisposition::Applied,
            events: events.len(),
            revision: self.state.revision,
        })
    }

    fn process_disconnect(
        &mut self,
        principal_id: PrincipalId,
        observation_id: CommandId,
    ) -> Result<AuthorityOutcome, InProcessAuthorityError<G::Error>> {
        let correlation = CorrelationId::new(format!("cor-{}", observation_id.as_str()))
            .map_err(|_| InProcessAuthorityError::InvalidIdentifier)?;
        let observation_hash = poche_protocol::SemanticHash(
            *blake3::hash(observation_id.as_str().as_bytes()).as_bytes(),
        );
        let events = decide_transport_disconnect(
            &self.state,
            &principal_id,
            &observation_id,
            observation_hash,
            &correlation,
        )
        .map_err(InProcessAuthorityError::Session)?;
        self.commit_events(&events)?;
        self.broadcast_projections(&correlation)?;
        Ok(AuthorityOutcome {
            principal_id,
            command_id: observation_id,
            disposition: AuthorityDisposition::Disconnected,
            events: events.len(),
            revision: self.state.revision,
        })
    }

    fn commit_events(
        &mut self,
        events: &[SessionEvent<G>],
    ) -> Result<(), InProcessAuthorityError<G::Error>> {
        let mut candidate = self.state.clone();
        for event in events {
            candidate = apply(&candidate, event).map_err(InProcessAuthorityError::Session)?;
        }
        self.state = candidate;
        for event in events {
            match &event.kind {
                SessionEventKind::CountdownArmed {
                    deadline_tick,
                    token,
                } => self
                    .clock
                    .arm_countdown(&self.state.room_id, *deadline_tick, token)
                    .unwrap_or_else(|never| match never {}),
                SessionEventKind::CountdownCancelled { token } => self
                    .clock
                    .cancel_countdown(&self.state.room_id, token)
                    .unwrap_or_else(|never| match never {}),
                _ => {}
            }
        }
        Ok(())
    }

    fn broadcast_projections(
        &mut self,
        correlation: &CorrelationId,
    ) -> Result<(), InProcessAuthorityError<G::Error>> {
        let recipients = self
            .state
            .members
            .iter()
            .filter(|member| member.connection == poche_session::ConnectionState::Connected)
            .map(|member| member.principal_id.clone())
            .collect::<Vec<_>>();
        for recipient in recipients {
            let payload = project_viewer(&self.state, &recipient, self.state.projection_epoch)
                .map_err(InProcessAuthorityError::Projection)?;
            let sequence = self.next_delivery;
            self.next_delivery = self.next_delivery.saturating_add(1);
            let frame = ProtocolFrame::Projection(ProjectionEnvelope {
                protocol_version: PROTOCOL_VERSION_V1,
                room_id: self.state.room_id.clone(),
                session_epoch: self.state.session_epoch,
                projection_id: ProjectionId::new(format!("projection-{sequence}"))
                    .map_err(|_| InProcessAuthorityError::InvalidIdentifier)?,
                principal_id: recipient.clone(),
                current_revision: self.state.revision,
                projection_epoch: self.state.projection_epoch,
                correlation_id: correlation.clone(),
                causation_id: EventId::new(format!("cause-{sequence}"))
                    .map_err(|_| InProcessAuthorityError::InvalidIdentifier)?,
                payload,
                signature: loopback_signature(&recipient)
                    .map_err(InProcessAuthorityError::Transport)?,
            });
            self.transport
                .send(&frame)
                .map_err(InProcessAuthorityError::Transport)?;
        }
        Ok(())
    }

    fn send_denial(
        &mut self,
        command: &CommandEnvelope,
        reason: DenyReason,
        policy_id: Option<poche_protocol::PolicyId>,
    ) -> Result<(), InProcessAuthorityError<G::Error>> {
        let sequence = self.next_delivery;
        self.next_delivery = self.next_delivery.saturating_add(1);
        let frame = ProtocolFrame::Error(ErrorEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: self.state.room_id.clone(),
            session_epoch: self.state.session_epoch,
            event_id: EventId::new(format!("error-{sequence}"))
                .map_err(|_| InProcessAuthorityError::InvalidIdentifier)?,
            principal_id: command.principal_id.clone(),
            current_revision: self.state.revision,
            correlation_id: command.correlation_id.clone(),
            causation_id: command.command_id.clone(),
            payload: ErrorPayload {
                reason,
                policy_id,
                public_detail: None,
            },
            signature: loopback_signature(&command.principal_id)
                .map_err(InProcessAuthorityError::Transport)?,
        });
        self.transport
            .send(&frame)
            .map_err(InProcessAuthorityError::Transport)
    }
}

fn decision_reason(decision: &PolicyDecision) -> Option<DenyReason> {
    match decision {
        PolicyDecision::Allow { .. } => None,
        PolicyDecision::Deny { reason, .. } => Some(*reason),
    }
}

fn decision_policy(decision: &PolicyDecision) -> Option<poche_protocol::PolicyId> {
    match decision {
        PolicyDecision::Allow { .. } => None,
        PolicyDecision::Deny { policy_id, .. } => policy_id.clone(),
    }
}

fn runtime_command<G: SessionGame>(
    state: &SessionState<G>,
    principal: &PrincipalId,
    command_id: &str,
    payload: CommandPayload,
) -> Result<CommandEnvelope, InProcessAuthorityError<G::Error>> {
    let command_id =
        CommandId::new(command_id).map_err(|_| InProcessAuthorityError::InvalidIdentifier)?;
    let correlation_id = CorrelationId::new(format!("cor-{}", command_id.as_str()))
        .map_err(|_| InProcessAuthorityError::InvalidIdentifier)?;
    Ok(UnsignedCommandEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id: state.room_id.clone(),
        session_epoch: state.session_epoch,
        command_id,
        principal_id: principal.clone(),
        expected_revision: state.revision,
        correlation_id,
        causation_id: None,
        payload,
        signature_intent: SignatureIntent {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: principal.clone(),
        },
    }
    .attach_signature(loopback_signature_bytes().map_err(InProcessAuthorityError::Transport)?))
}

fn loopback_signature(
    principal: &PrincipalId,
) -> Result<SignatureMetadata, InProcessTransportError> {
    Ok(SignatureMetadata {
        domain_version: SIGNATURE_DOMAIN_V1,
        algorithm: SignatureAlgorithm::Ed25519,
        key_id: principal.clone(),
        signature: loopback_signature_bytes()?,
    })
}

fn loopback_signature_bytes() -> Result<SignatureBytes, InProcessTransportError> {
    SignatureBytes::new("0".repeat(128)).map_err(|_| InProcessTransportError::InvalidIdentifier)
}

// Assert at compile time that orchestration uses the public pure-machine type.
const _: fn() = || {
    fn require_machine<M: PureSessionMachine>() {}
    require_machine::<SessionMachine<crate::OracleSessionGame<2>>>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OracleSessionGame;

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("test principal should be valid")
    }

    fn room(value: &str) -> RoomId {
        RoomId::new(value).expect("test room should be valid")
    }

    fn state() -> SessionState<OracleSessionGame<2>> {
        SessionState::pending(room("loopback-room"), principal("clock"), principal("game"))
    }

    fn command(
        client: &ScriptedClient,
        state: &SessionState<OracleSessionGame<2>>,
        id: &str,
    ) -> CommandEnvelope {
        client
            .command(state, id, CommandPayload::CreateRoom)
            .expect("test command should build")
    }

    #[test]
    fn typed_and_ndjson_ingress_are_semantically_identical() {
        let state = state();
        let mut typed = InProcessTransport::new(LoopbackCodec::Typed);
        let typed_client = typed.connect(principal("host")).unwrap();
        typed_client
            .submit(&mut typed, command(&typed_client, &state, "create"))
            .unwrap();

        let mut ndjson = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let ndjson_client = ndjson.connect(principal("host")).unwrap();
        ndjson_client
            .submit(&mut ndjson, command(&ndjson_client, &state, "create"))
            .unwrap();

        assert_eq!(typed.receive(), ndjson.receive());
    }

    #[test]
    fn ndjson_mode_round_trips_authority_egress() {
        let state = state();
        let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
        let client = transport.connect(principal("host")).unwrap();
        let command = command(&client, &state, "create");
        client.submit(&mut transport, command).unwrap();
        let mut authority = InProcessAuthority::new(state, transport);
        let outcomes = authority.drive_all().unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].disposition, AuthorityDisposition::Applied);
        assert!(matches!(
            client.receive(&mut authority.transport).unwrap(),
            Some(ProtocolFrame::Projection(_))
        ));
    }

    #[test]
    fn duplicate_delivery_is_idempotent_at_the_authority() {
        let state = state();
        let mut transport = InProcessTransport::default();
        let client = transport.connect(principal("host")).unwrap();
        transport.inject_fault(InProcessFault::DuplicateNext);
        let command = command(&client, &state, "create");
        client.submit(&mut transport, command).unwrap();
        let mut authority = InProcessAuthority::new(state, transport);
        let outcomes = authority.drive_all().unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.disposition == AuthorityDisposition::Applied)
        );
        assert_eq!(authority.state.revision, 1);
        assert_eq!(authority.state.processed_commands.len(), 1);
    }

    #[test]
    fn faults_have_exact_deterministic_queue_effects() {
        let state = state();
        let mut transport = InProcessTransport::default();
        let client = transport.connect(principal("host")).unwrap();

        transport.inject_fault(InProcessFault::DuplicateNext);
        client
            .submit(&mut transport, command(&client, &state, "duplicate"))
            .unwrap();
        assert_eq!(transport.receive(), transport.receive());

        transport.inject_fault(InProcessFault::ReorderNextPair);
        client
            .submit(&mut transport, command(&client, &state, "first"))
            .unwrap();
        assert!(transport.receive().is_none());
        client
            .submit(&mut transport, command(&client, &state, "second"))
            .unwrap();
        let second = transport.receive().expect("second should arrive first");
        let first = transport
            .receive()
            .expect("held first should arrive second");
        assert!(matches!(
            second,
            AuthenticatedIngress::Command { command, .. } if command.command_id.as_str() == "second"
        ));
        assert!(matches!(
            first,
            AuthenticatedIngress::Command { command, .. } if command.command_id.as_str() == "first"
        ));

        transport.inject_fault(InProcessFault::DisconnectNext);
        assert_eq!(
            client.submit(&mut transport, command(&client, &state, "dropped")),
            Err(InProcessTransportError::DisconnectedByFault)
        );
        assert!(matches!(
            transport.receive(),
            Some(AuthenticatedIngress::Disconnected { principal_id, .. })
                if principal_id.as_str() == "host"
        ));
    }

    #[test]
    fn connection_binding_rejects_principal_spoofing() {
        let state = state();
        let mut transport = InProcessTransport::default();
        let host = transport.connect(principal("host")).unwrap();
        let attacker = transport.connect(principal("attacker")).unwrap();
        let host_command = command(&host, &state, "create");
        assert_eq!(
            attacker.submit(&mut transport, host_command),
            Err(InProcessTransportError::PrincipalBindingMismatch)
        );
        assert!(transport.receive().is_none());
    }

    #[test]
    fn manual_clock_orders_and_cancels_without_wall_time() {
        let mut clock = ManualClock::default();
        let room = room("clock-room");
        let late = CountdownToken::new("late").unwrap();
        let early = CountdownToken::new("early").unwrap();
        clock.arm_countdown(&room, 20, &late).unwrap();
        clock.arm_countdown(&room, 10, &early).unwrap();
        assert!(clock.advance_to(9).is_empty());
        assert_eq!(clock.advance_to(10)[0].token, early);
        clock.cancel_countdown(&room, &late).unwrap();
        assert!(clock.advance_to(100).is_empty());
        assert_eq!(clock.now(), 100);
    }
}

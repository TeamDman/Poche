use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use poche_protocol::{
    CommandId, CommandPayload, CorrelationId, DenyReason, InviteProof, PROTOCOL_VERSION_V1,
    PrincipalId, RoomId, SIGNATURE_DOMAIN_V1, SignatureAlgorithm, SignatureIntent,
    UnsignedCommandEnvelope,
};
use poche_runtime::OracleSessionGame;
use poche_session::{
    AuthorizedCommand, InviteRecord, PolicyDecision, SessionError, SessionState, apply, authorize,
    decide,
};

use crate::{
    ApplicationIdentity, ApplicationPublicIdentity, ExplicitInsecureDevelopment,
    IdentityStoragePolicy, InsecureMemoryIdentityStore, RoomCode, RoomNetwork,
    verify_command_signature,
};

const RECORD_KEY: &str =
    "VLD0:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

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

fn identity() -> ApplicationIdentity {
    let store = InsecureMemoryIdentityStore::new(
        ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
    );
    block_on(ApplicationIdentity::load_or_create(
        &store,
        IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        ),
    ))
    .unwrap()
}

fn infrastructure_principal(label: &str) -> PrincipalId {
    PrincipalId::new(label).unwrap()
}

fn unsigned(
    identity: &ApplicationPublicIdentity,
    room_id: RoomId,
    session_epoch: u64,
    revision: u64,
    command_id: &str,
    payload: CommandPayload,
) -> UnsignedCommandEnvelope {
    UnsignedCommandEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id,
        session_epoch,
        command_id: CommandId::new(command_id).unwrap(),
        principal_id: identity.principal_id.clone(),
        expected_revision: revision,
        correlation_id: CorrelationId::new(format!("correlation-{command_id}")).unwrap(),
        causation_id: None,
        payload,
        signature_intent: SignatureIntent {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: identity.principal_id.clone(),
        },
    }
}

fn apply_allowed(
    state: &SessionState<OracleSessionGame<2>>,
    identity: &ApplicationIdentity,
    command: UnsignedCommandEnvelope,
) -> SessionState<OracleSessionGame<2>> {
    let public = identity.public();
    let signed = identity.sign_command(command).unwrap();
    assert_eq!(verify_command_signature(&signed, &public), Ok(()));
    let decision = authorize(state, &signed);
    let authorized = AuthorizedCommand::from_decision(signed, decision).unwrap();
    let events = decide(state, &authorized).unwrap();
    events
        .iter()
        .fold(state.clone(), |next, event| apply(&next, event).unwrap())
}

fn lobby(host: &ApplicationIdentity, room_id: &RoomId) -> SessionState<OracleSessionGame<2>> {
    let pending = SessionState::pending(
        room_id.clone(),
        infrastructure_principal("authority-clock"),
        infrastructure_principal("game-environment"),
    );
    apply_allowed(
        &pending,
        host,
        unsigned(
            &host.public(),
            room_id.clone(),
            pending.session_epoch,
            pending.revision,
            "create-room",
            CommandPayload::CreateRoom,
        ),
    )
}

fn invite_code(host: &ApplicationIdentity) -> RoomCode {
    RoomCode::issue(
        RoomNetwork::VeilidLocal,
        RECORD_KEY,
        host.public().principal_id,
        20_000,
        10_000,
    )
    .unwrap()
}

fn signed_redeem(
    state: &SessionState<OracleSessionGame<2>>,
    client: &ApplicationIdentity,
    room_id: RoomId,
    command_id: &str,
    proof: &str,
) -> poche_protocol::CommandEnvelope {
    client
        .sign_command(unsigned(
            &client.public(),
            room_id,
            state.session_epoch,
            state.revision,
            command_id,
            CommandPayload::RedeemInvite {
                invite: InviteProof::new(proof).unwrap(),
            },
        ))
        .unwrap()
}

fn assert_semantic_denial(
    state: &SessionState<OracleSessionGame<2>>,
    signed: poche_protocol::CommandEnvelope,
    reason: DenyReason,
) {
    let decision = authorize(state, &signed);
    let authorized = AuthorizedCommand::from_decision(signed, decision).unwrap();
    assert!(matches!(
        decide(state, &authorized),
        Err(SessionError::Denied(observed)) if observed == reason
    ));
}

#[test]
fn code_redemption_binds_stable_key_and_rejects_replay_cross_room_and_revocation() {
    let host = identity();
    let client = identity();
    let room_id = RoomId::new("native-redemption-room").unwrap();
    let code = invite_code(&host);
    let code_text = code.encode().unwrap();

    let mut state = lobby(&host, &room_id);
    state
        .invites
        .push(InviteRecord::new(code_text.expose(), 100).unwrap());
    let signed = signed_redeem(
        &state,
        &client,
        room_id.clone(),
        "redeem-first",
        code_text.expose(),
    );
    assert_eq!(verify_command_signature(&signed, &client.public()), Ok(()));
    state = apply_allowed(
        &state,
        &client,
        unsigned(
            &client.public(),
            room_id.clone(),
            state.session_epoch,
            state.revision,
            "redeem-first",
            CommandPayload::RedeemInvite {
                invite: InviteProof::new(code_text.expose()).unwrap(),
            },
        ),
    );
    assert_eq!(
        state
            .member(&client.public().principal_id)
            .map(|member| &member.principal_id),
        Some(&client.public().principal_id)
    );

    let replay = signed_redeem(
        &state,
        &identity(),
        room_id.clone(),
        "redeem-replay",
        code_text.expose(),
    );
    assert_semantic_denial(&state, replay, DenyReason::Revoked);

    let mut cross_room = lobby(&host, &room_id);
    cross_room
        .invites
        .push(InviteRecord::new(code_text.expose(), 100).unwrap());
    let wrong_room = signed_redeem(
        &cross_room,
        &identity(),
        RoomId::new("different-room").unwrap(),
        "redeem-cross-room",
        code_text.expose(),
    );
    assert!(matches!(
        authorize(&cross_room, &wrong_room),
        PolicyDecision::Deny {
            reason: DenyReason::WrongRoom,
            ..
        }
    ));

    cross_room.invites[0].revoked = true;
    let revoked = signed_redeem(
        &cross_room,
        &identity(),
        room_id,
        "redeem-revoked",
        code_text.expose(),
    );
    assert_semantic_denial(&cross_room, revoked, DenyReason::Revoked);
}

#[test]
fn invalid_and_expired_codes_fail_before_membership() {
    let host = identity();
    let code = invite_code(&host);
    let text = code.encode().unwrap();
    assert!(RoomCode::decode(text.expose(), 20_000).is_err());

    let room_id = RoomId::new("invalid-code-room").unwrap();
    let mut state = lobby(&host, &room_id);
    state
        .invites
        .push(InviteRecord::new(text.expose(), 100).unwrap());
    let invalid = signed_redeem(
        &state,
        &identity(),
        room_id,
        "redeem-invalid",
        "p3-invalid-but-bounded",
    );
    assert_semantic_denial(&state, invalid, DenyReason::InviteInvalid);
}

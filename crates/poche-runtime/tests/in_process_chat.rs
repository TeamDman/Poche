use poche_protocol::{
    CodecError, CommandPayload, DenyReason, EnvelopeValidationError, InviteProof, PrincipalId,
    RoomId,
};
use poche_runtime::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessFault, InProcessTransport,
    InProcessTransportError, LoopbackCodec, OracleSessionGame, ScriptedClient,
};
use poche_session::{InviteRecord, SessionState};

fn principal(value: &str) -> PrincipalId {
    PrincipalId::new(value).unwrap()
}

fn submit(
    authority: &mut InProcessAuthority<OracleSessionGame<2>>,
    client: &ScriptedClient,
    command_id: &str,
    payload: CommandPayload,
) -> AuthorityDisposition {
    let command = client
        .command(&authority.state, command_id, payload)
        .unwrap();
    client.submit(&mut authority.transport, command).unwrap();
    let outcomes = authority.drive_all().unwrap();
    outcomes.last().unwrap().disposition.clone()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one inspectable chat security scenario preserves rejection precedence and tail chronology"
)]
fn chat_tail_is_bounded_attributed_redacted_and_fail_closed() {
    let mut state = SessionState::pending(
        RoomId::new("chat-room").unwrap(),
        principal("clock"),
        principal("game"),
    );
    state
        .invites
        .push(InviteRecord::new("member-invite-secret", u64::MAX).unwrap());
    let mut transport = InProcessTransport::new(LoopbackCodec::CanonicalNdjson);
    let host = transport.connect(principal("host")).unwrap();
    let member = transport.connect(principal("member")).unwrap();
    let outsider = transport.connect(principal("outsider")).unwrap();
    let mut authority = InProcessAuthority::with_chat_capacity(state, transport, 3);

    assert_eq!(
        submit(&mut authority, &host, "create", CommandPayload::CreateRoom),
        AuthorityDisposition::Applied
    );
    assert_eq!(
        submit(
            &mut authority,
            &member,
            "join-member",
            CommandPayload::RedeemInvite {
                invite: InviteProof::new("member-invite-secret").unwrap(),
            },
        ),
        AuthorityDisposition::Applied
    );

    for index in 0..5 {
        assert_eq!(
            submit(
                &mut authority,
                &host,
                &format!("chat-{index}"),
                CommandPayload::Chat {
                    text: format!("message-{index}"),
                },
            ),
            AuthorityDisposition::Applied
        );
    }
    assert_eq!(authority.chat_tail().len(), 3);
    assert_eq!(
        authority
            .chat_tail()
            .entries()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>(),
        ["message-2", "message-3", "message-4"]
    );

    assert_eq!(
        submit(
            &mut authority,
            &host,
            "chat-rate",
            CommandPayload::Chat {
                text: "sixth".to_owned(),
            },
        ),
        AuthorityDisposition::Denied(DenyReason::ChatRate)
    );
    assert_eq!(
        submit(
            &mut authority,
            &outsider,
            "chat-outsider",
            CommandPayload::Chat {
                text: "outsider".to_owned(),
            },
        ),
        AuthorityDisposition::Denied(DenyReason::UnknownPrincipal)
    );
    let oversize = member
        .command(
            &authority.state,
            "chat-oversize",
            CommandPayload::Chat {
                text: "x".repeat(2_049),
            },
        )
        .unwrap();
    assert_eq!(
        member.submit(&mut authority.transport, oversize),
        Err(InProcessTransportError::Codec(CodecError::InvalidEnvelope(
            EnvelopeValidationError::InvalidPayload
        )))
    );

    let injected = "hello\n{\"frame\":\"command\"}\rworld";
    authority
        .transport
        .inject_fault(InProcessFault::DuplicateNext);
    assert_eq!(
        submit(
            &mut authority,
            &member,
            "chat-control",
            CommandPayload::Chat {
                text: injected.to_owned(),
            },
        ),
        AuthorityDisposition::Applied
    );
    assert_eq!(
        authority
            .chat_tail()
            .entries()
            .filter(|entry| entry.text == injected)
            .count(),
        1,
        "duplicate transport delivery must not duplicate the chat side stream"
    );

    let export = authority.chat_tail().export_ndjson().unwrap();
    assert!(!export.contains("chat-control"));
    assert!(export.contains("hello\\n{\\\"frame\\\":\\\"command\\\"}\\rworld"));
    assert!(!export.contains("member-invite-secret"));
    assert!(!export.contains("signature"));
    assert!(!export.contains("own_hand"));
    assert!(
        export
            .lines()
            .all(|line| serde_json::from_str::<poche_runtime::ChatEntry>(line).is_ok())
    );

    assert_eq!(
        submit(&mut authority, &host, "close", CommandPayload::CloseRoom),
        AuthorityDisposition::Applied
    );
    assert_eq!(
        submit(
            &mut authority,
            &member,
            "chat-closed",
            CommandPayload::Chat {
                text: "closed".to_owned(),
            },
        ),
        AuthorityDisposition::Denied(DenyReason::Closed)
    );
}

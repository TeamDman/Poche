use poche_environment::{
    GameEnvironment, OracleChanceAction, OracleEnvironment, OraclePlayerAction,
};
use poche_oracle_rust::{Card, round_count};
use poche_protocol::{
    ChanceWire, CommandPayload, CountdownToken, GameActionWire, InviteProof, PrincipalId, RoomId,
};
use poche_runtime::{
    AuthorityDisposition, ClientPort, InProcessAuthority, InProcessTransport, LoopbackCodec,
    OracleSessionGame, ScriptedClient,
};
use poche_session::{GameTurn, InviteRecord, SessionGame, SessionPhase, SessionState};

fn principal(value: &str) -> PrincipalId {
    PrincipalId::new(value).expect("test principal should be valid")
}

fn submit(
    authority: &mut InProcessAuthority<OracleSessionGame<2>>,
    client: &ScriptedClient,
    command_id: &str,
    payload: CommandPayload,
) {
    let command = client
        .command(&authority.state, command_id, payload)
        .expect("command should build");
    client
        .submit(&mut authority.transport, command)
        .expect("transport should accept command");
    let outcomes = authority
        .drive_all()
        .expect("authority should drive command");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].disposition, AuthorityDisposition::Applied);
}

fn chance_wire(seed: u64, deal_ordinal: u32) -> ChanceWire {
    let chance = OracleChanceAction::seeded(seed, deal_ordinal);
    let standard = Card::standard_deck();
    let cards = chance
        .deck
        .cards()
        .iter()
        .map(|card| {
            standard
                .iter()
                .position(|candidate| candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .expect("seeded deck card should have a canonical code")
        })
        .collect();
    ChanceWire {
        cards,
        seed: Some(seed),
        deal_ordinal: Some(deal_ordinal),
    }
}

fn action_wire(action: OraclePlayerAction<2>) -> GameActionWire {
    match action {
        OraclePlayerAction::Bid { tricks, .. } => GameActionWire::Bid { tricks },
        OraclePlayerAction::Play { card, .. } => {
            let standard = Card::standard_deck();
            let card = standard
                .iter()
                .position(|candidate| *candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .expect("legal card should have a canonical code");
            GameActionWire::Play { card }
        }
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the acceptance scenario deliberately keeps the complete lifecycle and game in one inspectable script"
)]
fn three_scripted_clients_complete_a_real_poche_game_without_sockets() {
    let host_id = principal("host");
    let player_id = principal("alice");
    let spectator_id = principal("spectator");
    let clock_id = principal("authority-clock");
    let game_id = principal("game-environment");
    let mut state = SessionState::pending(
        RoomId::new("full-game-room").unwrap(),
        clock_id,
        game_id.clone(),
    );
    state
        .invites
        .push(InviteRecord::new("alice-invite", u64::MAX).unwrap());
    state
        .invites
        .push(InviteRecord::new("spectator-invite", u64::MAX).unwrap());

    let mut transport = InProcessTransport::new(LoopbackCodec::Typed);
    let host = transport.connect(host_id).unwrap();
    let player = transport.connect(player_id).unwrap();
    let spectator = transport.connect(spectator_id).unwrap();
    let game_environment = transport.connect(game_id).unwrap();
    let mut authority = InProcessAuthority::new(state, transport);

    submit(&mut authority, &host, "create", CommandPayload::CreateRoom);
    submit(
        &mut authority,
        &player,
        "join-alice",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new("alice-invite").unwrap(),
        },
    );
    submit(
        &mut authority,
        &spectator,
        "join-spectator",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new("spectator-invite").unwrap(),
        },
    );
    submit(
        &mut authority,
        &host,
        "seat-host",
        CommandPayload::TakeSeat { seat: 0 },
    );
    submit(
        &mut authority,
        &player,
        "seat-alice",
        CommandPayload::TakeSeat { seat: 1 },
    );
    submit(&mut authority, &host, "ready-host", CommandPayload::Ready);
    submit(
        &mut authority,
        &player,
        "ready-alice",
        CommandPayload::Ready,
    );
    submit(
        &mut authority,
        &host,
        "arm-countdown",
        CommandPayload::ArmCountdown {
            deadline_tick: 5,
            countdown_token: CountdownToken::new("start-token").unwrap(),
        },
    );
    let expiry = authority.advance_clock_to(5).expect("expiry should apply");
    assert_eq!(expiry.len(), 1);
    assert!(matches!(
        authority.state.phase,
        SessionPhase::Running { .. }
    ));

    let mut deal_ordinal = 0_u32;
    let mut command_sequence = 0_u32;
    while !matches!(authority.state.phase, SessionPhase::PostGame { .. }) {
        assert!(
            command_sequence < 1_000,
            "real game should terminate within a bounded script"
        );
        let (turn, action) = match &authority.state.phase {
            SessionPhase::Running { game } => {
                let turn = game.turn();
                let action = match turn {
                    GameTurn::Player(_) => Some(
                        OracleEnvironment::<2>::legal_actions(&game.game)
                            .into_iter()
                            .next()
                            .expect("acting player should have a legal action"),
                    ),
                    GameTurn::Chance | GameTurn::Environment | GameTurn::Finished => None,
                };
                (turn, action)
            }
            phase => panic!("unexpected phase during full game: {phase:?}"),
        };
        let command_id = format!("game-{command_sequence}");
        match turn {
            GameTurn::Chance => {
                submit(
                    &mut authority,
                    &game_environment,
                    &command_id,
                    CommandPayload::ApplyChance {
                        chance: chance_wire(0x5eed, deal_ordinal),
                    },
                );
                deal_ordinal += 1;
            }
            GameTurn::Player(seat) => {
                let client = if seat == 0 { &host } else { &player };
                submit(
                    &mut authority,
                    client,
                    &command_id,
                    CommandPayload::GameAction {
                        action: action_wire(action.expect("player action should exist")),
                    },
                );
            }
            GameTurn::Environment => submit(
                &mut authority,
                &game_environment,
                &command_id,
                CommandPayload::Settle,
            ),
            GameTurn::Finished => panic!("session should enter post-game on terminal transition"),
        }
        command_sequence += 1;
    }

    assert_eq!(
        usize::try_from(deal_ordinal).expect("deal count should fit usize"),
        round_count(2),
        "the session should preserve the oracle's exact two-player schedule"
    );
    assert!(command_sequence > 10);
    for client in [&host, &player, &spectator] {
        let mut projections = 0;
        while let Some(frame) = client.receive(&mut authority.transport).unwrap() {
            assert!(matches!(
                frame,
                poche_protocol::ProtocolFrame::Projection(_)
            ));
            projections += 1;
        }
        assert!(
            projections > 10,
            "every client should receive scoped progress"
        );
    }
}

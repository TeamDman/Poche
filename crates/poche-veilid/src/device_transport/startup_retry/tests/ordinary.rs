use super::*;

#[test]
fn ordinary_commands_recover_lost_requests_and_receipts_without_reapplication() {
    for after_dispatch in [false, true] {
        let h = Harness::new();
        let (creator, signer) = profile(101);
        for (id, payload) in [
            ("create", CommandPayload::CreateRoom),
            ("seat", CommandPayload::TakeSeat { seat: 0 }),
            ("ready", CommandPayload::Ready),
            ("unready", CommandPayload::Unready),
            ("release", CommandPayload::ReleaseSeat),
            (
                "chat",
                CommandPayload::Chat {
                    text: "one message".into(),
                },
            ),
            ("close", CommandPayload::CloseRoom),
        ] {
            let (prepared, request) = h.action(&creator, &signer, &payload, id);
            h.commit_with_loss(&prepared, &request, after_dispatch);
        }
        let view = h.adapter.clone().observe(&creator, &h.room).unwrap();
        assert_eq!(
            view.chat_tail.len(),
            1,
            "a lost reply must not duplicate chat"
        );
        assert_eq!(view.projection.payload.phase, RoomPhase::Closed);
        assert_eq!(view.projection.current_revision, 7);
    }
}

#[test]
fn game_bid_and_play_recover_loss_without_duplicate_history_or_extra_reveal() {
    for after_dispatch in [false, true] {
        let h = Harness::new();
        let (creator, creator_signer) = profile(101);
        let (guest, guest_signer) = profile(103);
        let (clock, clock_signer) = profile(11);
        let (game, game_signer) = profile(13);
        for (player, signer, id, payload) in [
            (
                &creator,
                &creator_signer,
                "create",
                CommandPayload::CreateRoom,
            ),
            (
                &guest,
                &guest_signer,
                "join",
                CommandPayload::RedeemInvite {
                    invite: InviteProof::new("startup-invite").unwrap(),
                },
            ),
            (
                &creator,
                &creator_signer,
                "seat-0",
                CommandPayload::TakeSeat { seat: 0 },
            ),
            (
                &guest,
                &guest_signer,
                "seat-1",
                CommandPayload::TakeSeat { seat: 1 },
            ),
            (&creator, &creator_signer, "ready-0", CommandPayload::Ready),
            (&guest, &guest_signer, "ready-1", CommandPayload::Ready),
            (
                &creator,
                &creator_signer,
                "arm",
                CommandPayload::ArmCountdown {
                    deadline_tick: 30,
                    countdown_token: CountdownToken::new("startup-countdown").unwrap(),
                },
            ),
            (
                &clock,
                &clock_signer,
                "expire",
                CommandPayload::CountdownExpired {
                    countdown_token: CountdownToken::new("startup-countdown").unwrap(),
                },
            ),
            (
                &game,
                &game_signer,
                "deal",
                CommandPayload::ApplyChance {
                    chance: OracleSessionGame::<2>::seeded_chance(29, 0).unwrap(),
                },
            ),
        ] {
            let (prepared, request) = h.action(player, signer, &payload, id);
            h.commit_with_loss(&prepared, &request, after_dispatch);
        }
        // Select from the actual current actor's advertised choices, never
        // bypass the reducer to manufacture a turn or card identity.
        for index in 0..3 {
            let (player, signer, action) = [(&creator, &creator_signer), (&guest, &guest_signer)]
                .into_iter()
                .find_map(|(player, signer)| {
                    h.adapter
                        .clone()
                        .observe(player, &h.room)
                        .unwrap()
                        .actions
                        .into_iter()
                        .find(|action| matches!(action.payload, CommandPayload::GameAction { .. }))
                        .map(|action| (player, signer, action))
                })
                .expect("one player has a legal bid/play");
            assert!(matches!(
                (&action.payload, index),
                (
                    CommandPayload::GameAction {
                        action: GameActionWire::Bid { .. }
                    },
                    0 | 1
                ) | (
                    CommandPayload::GameAction {
                        action: GameActionWire::Play { .. }
                    },
                    2
                )
            ));
            let (prepared, request) =
                h.action(player, signer, &action.payload, &format!("game-{index}"));
            h.commit_with_loss(&prepared, &request, after_dispatch);
        }
        let a = h.adapter.clone().observe(&creator, &h.room).unwrap();
        let b = h.adapter.clone().observe(&guest, &h.room).unwrap();
        assert_eq!(a.projection.current_revision, 12);
        assert_eq!(
            a.projection.payload.public_history,
            b.projection.payload.public_history
        );
        assert_eq!(
            a.projection
                .payload
                .public_history
                .iter()
                .filter(|event| matches!(event, PublicGameEventWire::PlayerAction { .. }))
                .count(),
            3
        );
        let public = a.projection.payload.public_game_state.as_ref().unwrap();
        assert_eq!(
            public.current_trick.len(),
            1,
            "only the played card is public"
        );
        assert!(public.bids.iter().all(Option::is_some));
        assert_eq!(
            a.projection.payload.own_hand.as_ref().unwrap().cards.len()
                + b.projection.payload.own_hand.as_ref().unwrap().cards.len(),
            1
        );
    }
}

#[test]
fn exhausted_ordinary_receipt_retries_report_unknown_not_rejection_or_new_action() {
    let h = Harness::new();
    let (creator, signer) = profile(111);
    let (_, create) = h.action(&creator, &signer, &CommandPayload::CreateRoom, "create");
    h.service.dispatch(&create.encode().unwrap()).unwrap();
    let (prepared, seat) = h.action(
        &creator,
        &signer,
        &CommandPayload::TakeSeat { seat: 0 },
        "seat",
    );
    let frozen = seat.encode().unwrap();
    let mut calls = 0;
    let mut pauses = Vec::new();
    assert_eq!(
        exchange(
            &seat,
            |bytes, refresh| {
                assert!(!refresh);
                assert!(bytes == frozen);
                calls += 1;
                h.service.dispatch(bytes).unwrap();
                Err(VeilidRendezvousError::Timeout) // All three replies are lost.
            },
            |delay| pauses.push(delay)
        ),
        Err(DeviceClientError::TransportUnavailable)
    );
    assert_eq!(calls, 3);
    assert_eq!(
        pauses,
        [Duration::from_millis(250), Duration::from_millis(500)]
    );
    assert_eq!(h.adapter.revision(), Some(2));
    let VeilidDeviceReply::Action(result) =
        VeilidDeviceReply::decode(&h.service.dispatch(&frozen).unwrap()).unwrap()
    else {
        panic!("cached receipt");
    };
    validate_result(&prepared, &result).unwrap();
    assert!(matches!(
        result,
        DeviceActionResult::Committed { revision: 2, .. }
    ));
    assert_eq!(h.adapter.revision(), Some(2));
}

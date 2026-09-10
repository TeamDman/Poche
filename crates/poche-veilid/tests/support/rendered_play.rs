//! Signed mock-service setup is shared with ordinary integration acceptance.
//! Only setup/bidding is scripted; both play attempts are rendered pointer input.
use super::*;
use poche_native_ui::input_probe::rendered::{ExpectedPlay, drag_to_play};
use poche_player_client::{DeviceActionResult, PlayerDeviceClient};
use poche_veilid::{VeilidDeviceNode, VeilidDeviceTransport};
type Client = PlayerDeviceClient<VeilidDeviceTransport<TestSigner>>;

pub(super) fn exercise(
    mut creator: Client,
    mut guest: Client,
    creator_node: VeilidDeviceNode,
    guest_node: VeilidDeviceNode,
    invitation: &str,
    room: &RoomId,
) {
    let root = std::path::PathBuf::from(
        std::env::var_os("POCHE_RENDERED_EVIDENCE_ROOT")
            .expect("set a fresh POCHE_RENDERED_EVIDENCE_ROOT"),
    );
    std::fs::create_dir(&root).expect("fresh renderer evidence root");
    for index in 0..2 {
        let view = creator.observe(room).unwrap();
        let creator_turn = view.actions.iter().any(|action| {
            matches!(
                action.payload,
                CommandPayload::GameAction {
                    action: GameActionWire::Bid { .. }
                }
            )
        });
        let client = if creator_turn {
            &mut creator
        } else {
            &mut guest
        };
        let view = client.observe(room).unwrap();
        let action = view
            .actions
            .iter()
            .find(|action| {
                matches!(
                    action.payload,
                    CommandPayload::GameAction {
                        action: GameActionWire::Bid { .. }
                    }
                )
            })
            .unwrap();
        assert!(matches!(
            client
                .invoke(
                    &view,
                    &action.id,
                    CommandId::new(format!("render-bid-{index}")).unwrap()
                )
                .unwrap(),
            DeviceActionResult::Committed { .. }
        ));
    }
    let view = creator.observe(room).unwrap();
    let creator_turn = view.actions.iter().any(|action| {
        matches!(
            action.payload,
            CommandPayload::GameAction {
                action: GameActionWire::Play { .. }
            }
        )
    });
    for (expected, as_creator, folder) in [
        (ExpectedPlay::Denied, !creator_turn, "denied"),
        (ExpectedPlay::Accepted, creator_turn, "accepted"),
    ] {
        let (root_seed, device_seed, node) = if as_creator {
            (31, 35, creator_node.clone())
        } else {
            (41, 45, guest_node.clone())
        };
        let (client, _) = poche_veilid::join_device(
            node,
            test_profile_for_device(root_seed, device_seed),
            TestSigner(SigningKey::from_bytes(&[device_seed; 32])),
            invitation,
            105,
        )
        .unwrap();
        let live = poche_native_ui::NativeLiveDevice::connect(client, room.clone()).unwrap();
        let before = live.observation().projection.current_revision;
        let result = drag_to_play(live, expected, &root.join(folder)).unwrap();
        let peer = if as_creator { &mut guest } else { &mut creator };
        let peer = peer.observe(room).unwrap();
        assert_eq!(
            peer.projection.current_revision,
            result.live.observation().projection.current_revision
        );
        if expected == ExpectedPlay::Denied {
            assert_eq!(peer.projection.current_revision, before);
            let card = peer
                .physical_hands
                .iter()
                .find(|card| card.id == result.card_id)
                .unwrap();
            assert!(
                card.face.is_none(),
                "denied play revealed the face to another player"
            );
            let pose = card.pose.as_ref().unwrap();
            assert_eq!(pose.position_mm, result.position_mm);
            assert_eq!(pose.rotation_millidegrees, result.rotation_millidegrees);
            assert!(
                !peer
                    .physical_public
                    .iter()
                    .any(|card| card.id == result.card_id)
            );
        } else {
            assert!(peer.projection.current_revision > before);
            assert!(
                !peer
                    .physical_hands
                    .iter()
                    .any(|card| card.id == result.card_id)
            );
            let card = peer
                .physical_public
                .iter()
                .find(|card| card.id == result.card_id)
                .unwrap();
            assert_eq!(card.pose.position_mm, result.position_mm);
            assert_eq!(
                card.pose.rotation_millidegrees,
                result.rotation_millidegrees
            );
        }
        result.live.shutdown().unwrap();
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_protocol::{CommandPayload, GameActionWire};
use poche_spatial::{
    AabbMm, CardFace, CardObjectId, InteractionFinding, ResolvedCardPlay, SeatId, SpatialLayout,
    SpatialPlayRecord, SpatialScene, resolve_card_play, resolve_drag_play,
};

/// Canonical game payload plus presentation-only replay sidecar resolved from
/// one viewer interaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialPlayCommand {
    /// Existing canonical protocol payload sent through every transport.
    pub payload: CommandPayload,
    /// Semantic endpoints/easing retained for renderer replay, never authority.
    pub spatial_record: SpatialPlayRecord,
}

impl From<ResolvedCardPlay> for SpatialPlayCommand {
    fn from(value: ResolvedCardPlay) -> Self {
        Self {
            payload: CommandPayload::GameAction {
                action: GameActionWire::Play {
                    card: value.face.code(),
                },
            },
            spatial_record: value.record,
        }
    }
}

/// Resolve a typed face selection into the canonical protocol command.
///
/// # Errors
///
/// Returns the stable engine-neutral interaction finding without changing
/// scene or authority state.
pub fn resolve_spatial_card_command(
    layout: &SpatialLayout,
    scene: &SpatialScene,
    issuing_seat: SeatId,
    face: CardFace,
    legal_actions: &[GameActionWire],
) -> Result<SpatialPlayCommand, InteractionFinding> {
    let resolved = resolve_card_play(layout, scene, issuing_seat, face)?;
    checked_command(resolved.into(), legal_actions)
}

/// Resolve a drag release into the same canonical protocol command.
///
/// # Errors
///
/// Returns the stable engine-neutral interaction finding without changing
/// scene or authority state.
pub fn resolve_spatial_drag_command(
    layout: &SpatialLayout,
    scene: &SpatialScene,
    issuing_seat: SeatId,
    object: CardObjectId,
    released_bounds: AabbMm,
    legal_actions: &[GameActionWire],
) -> Result<SpatialPlayCommand, InteractionFinding> {
    let resolved = resolve_drag_play(layout, scene, issuing_seat, object, released_bounds)?;
    checked_command(resolved.into(), legal_actions)
}

fn checked_command(
    command: SpatialPlayCommand,
    legal_actions: &[GameActionWire],
) -> Result<SpatialPlayCommand, InteractionFinding> {
    let CommandPayload::GameAction { action } = &command.payload else {
        return Err(InteractionFinding::IllegalAction);
    };
    if legal_actions.contains(action) {
        Ok(command)
    } else {
        Err(InteractionFinding::IllegalAction)
    }
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        ChanceWire, CommandId, CommandPayload, CorrelationId, GameActionWire, PROTOCOL_VERSION_V1,
        PrincipalId, RoomId, SIGNATURE_DOMAIN_V1, SignatureAlgorithm, SignatureBytes,
        SignatureIntent, UnsignedCommandEnvelope, decode_command_line, encode_command_line,
        verified_command_semantic_hash,
    };
    use poche_session::{GameTurn, SessionGame};
    use poche_spatial::{
        AabbMm, CardFace, HalfExtentsMm, InteractionFinding, LayoutId, PlayedCardProjection,
        PlayerSpatialProjection, Point3Mm, SeatId, TableId, ViewerSpatialProjection, ZoneId,
        realize_viewer_scene, registered_layout,
    };

    use crate::{OracleSessionGame, resolve_spatial_card_command, resolve_spatial_drag_command};

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("principal")
    }

    fn playing_game() -> OracleSessionGame<2> {
        let game = OracleSessionGame::<2>::start(&[(0, principal("alice")), (1, principal("bob"))])
            .expect("game");
        let mut cards = (0_u8..52).collect::<Vec<_>>();
        cards.swap(0, 48);
        let game = game
            .chance_transition(&ChanceWire {
                cards,
                seed: None,
                deal_ordinal: None,
            })
            .expect("deal")
            .game;
        let GameTurn::Player(first) = game.turn() else {
            panic!("first bidder");
        };
        let game = game
            .player_transition(first, &GameActionWire::Bid { tricks: 0 })
            .expect("first bid")
            .game;
        let GameTurn::Player(second) = game.turn() else {
            panic!("second bidder");
        };
        game.player_transition(second, &GameActionWire::Bid { tricks: 0 })
            .expect("second bid")
            .game
    }

    fn scene_for(
        game: &OracleSessionGame<2>,
    ) -> (
        poche_spatial::SpatialLayout,
        poche_spatial::SpatialScene,
        SeatId,
        CardFace,
    ) {
        let public = game.public_projection().expect("public projection");
        let GameTurn::Player(actor) = game.turn() else {
            panic!("playing actor");
        };
        let layout_id = LayoutId::new(2, 1).expect("layout ID");
        let layout = registered_layout(TableId::new(31), layout_id).expect("layout");
        let actor_seat = SeatId::new(actor, layout_id).expect("actor seat");
        let action = game
            .legal_player_actions()
            .into_iter()
            .find_map(|action| match action {
                GameActionWire::Play { card } => Some(card),
                GameActionWire::Bid { .. } => None,
            })
            .expect("legal card");
        let players = (0_u8..2)
            .map(|ordinal| {
                let seat = SeatId::new(ordinal, layout_id).expect("seat");
                let visible_hand = (ordinal == actor).then(|| {
                    game.private_hand(ordinal)
                        .expect("private hand")
                        .into_iter()
                        .map(|code| CardFace::new(code).expect("card"))
                        .collect()
                });
                PlayerSpatialProjection {
                    seat,
                    display_name: if ordinal == 0 { "alice" } else { "bob" }.to_owned(),
                    score: public.scores[usize::from(ordinal)],
                    hand_count: public.hand_counts[usize::from(ordinal)],
                    visible_hand,
                    tricks_won: public.tricks_won[usize::from(ordinal)],
                }
            })
            .collect();
        let projection = ViewerSpatialProjection {
            projection_epoch: 8,
            players,
            trump: public.trump.map(|code| CardFace::new(code).expect("trump")),
            current_trick: public
                .current_trick
                .into_iter()
                .map(|played| PlayedCardProjection {
                    seat: SeatId::new(played.seat, layout_id).expect("played seat"),
                    face: CardFace::new(played.card).expect("played card"),
                })
                .collect(),
            revealed_won_cards: Vec::new(),
        };
        let scene = realize_viewer_scene(&layout, &projection).expect("scene");
        (
            layout,
            scene,
            actor_seat,
            CardFace::new(action).expect("action face"),
        )
    }

    fn play_zone_bounds(layout: &poche_spatial::SpatialLayout) -> AabbMm {
        let play = layout
            .zones()
            .iter()
            .find(|zone| zone.id == ZoneId::Play)
            .expect("play zone");
        let center = Point3Mm::new(
            i32::midpoint(play.inner.min.x.get(), play.inner.max.x.get()),
            i32::midpoint(play.inner.min.y.get(), play.inner.max.y.get()),
            i32::midpoint(play.inner.min.z.get(), play.inner.max.z.get()),
        );
        AabbMm::from_center(center, HalfExtentsMm::new(32, 1, 44)).expect("card bounds")
    }

    fn signed(payload: CommandPayload) -> poche_protocol::CommandEnvelope {
        UnsignedCommandEnvelope {
            protocol_version: PROTOCOL_VERSION_V1,
            room_id: RoomId::new("spatial-room").expect("room"),
            session_epoch: 1,
            command_id: CommandId::new("play-card").expect("command"),
            principal_id: principal("alice"),
            expected_revision: 7,
            correlation_id: CorrelationId::new("cor-play-card").expect("correlation"),
            causation_id: None,
            payload,
            signature_intent: SignatureIntent {
                domain_version: SIGNATURE_DOMAIN_V1,
                algorithm: SignatureAlgorithm::Ed25519,
                key_id: principal("alice"),
            },
        }
        .attach_signature(SignatureBytes::new("0".repeat(128)).expect("signature"))
    }

    fn successor_hash(game: &OracleSessionGame<2>) -> [u8; 32] {
        let view = (
            game.public_projection().expect("public"),
            game.private_hand(0).expect("hand zero"),
            game.private_hand(1).expect("hand one"),
        );
        *blake3::hash(&serde_json::to_vec(&view).expect("semantic view")).as_bytes()
    }

    #[test]
    fn spatial_typed_drag_and_ndjson_paths_have_identical_successors_and_hashes() {
        let game = playing_game();
        let (layout, scene, actor, face) = scene_for(&game);
        assert_eq!(face.code(), 48, "fixture action is jack-spades");
        let legal = game.legal_player_actions();
        let typed =
            resolve_spatial_card_command(&layout, &scene, actor, face, &legal).expect("typed");
        let object = typed.spatial_record.object;
        let dragged = resolve_spatial_drag_command(
            &layout,
            &scene,
            actor,
            object,
            play_zone_bounds(&layout),
            &legal,
        )
        .expect("dragged");
        assert_eq!(typed, dragged);

        let signed = signed(typed.payload.clone());
        let line = encode_command_line(&signed).expect("encode NDJSON");
        let decoded = decode_command_line(&line).expect("decode NDJSON");
        assert_eq!(decoded.payload, typed.payload);
        assert_eq!(
            verified_command_semantic_hash(&signed).expect("typed hash"),
            verified_command_semantic_hash(&decoded).expect("NDJSON hash")
        );

        let CommandPayload::GameAction { action } = typed.payload else {
            panic!("game action");
        };
        let CommandPayload::GameAction {
            action: decoded_action,
        } = decoded.payload
        else {
            panic!("decoded game action");
        };
        let GameTurn::Player(actor_ordinal) = game.turn() else {
            panic!("player actor");
        };
        let typed_successor = game
            .player_transition(actor_ordinal, &action)
            .expect("typed successor")
            .game;
        let drag_successor = game
            .player_transition(actor_ordinal, &decoded_action)
            .expect("drag/NDJSON successor")
            .game;
        assert_eq!(typed_successor, drag_successor);
        assert_eq!(
            successor_hash(&typed_successor),
            successor_hash(&drag_successor)
        );
        assert_eq!(
            resolve_spatial_card_command(&layout, &scene, actor, face, &[]),
            Err(InteractionFinding::IllegalAction)
        );
    }
}

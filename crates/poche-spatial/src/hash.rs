// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Canonical fingerprinting for exact-recipient spatial scenes.

use core::fmt::Write as _;

use crate::{
    CardLocation, ObjectId, Point3Mm, PoseMm, SceneError, SceneObjectKind, SpatialScene, SurfaceId,
    SurfaceKind, TextBinding, ZoneId,
};

/// Hash one validated scene using an explicit, versioned field encoding.
///
/// The hash is renderer-independent. Vector order is significant because the
/// realization contract emits objects, cards, and text in canonical order.
///
/// # Errors
///
/// Returns the scene's structural/privacy validation failure before hashing.
pub fn spatial_scene_hash(scene: &SpatialScene) -> Result<[u8; 32], SceneError> {
    scene.validate()?;
    let mut hash = blake3::Hasher::new();
    hash.update(b"poche-spatial-scene-hash-v1\0");
    u16_field(&mut hash, scene.schema_version);
    u64_field(&mut hash, scene.table_id.get());
    u8_field(&mut hash, scene.layout.players());
    u16_field(&mut hash, scene.layout.revision());
    u64_field(&mut hash, scene.projection_epoch);

    len_field(&mut hash, scene.objects.len());
    for object in &scene.objects {
        object_id(&mut hash, object.id);
        u8_field(
            &mut hash,
            match object.kind {
                SceneObjectKind::Table => 0,
                SceneObjectKind::Seat => 1,
                SceneObjectKind::Player => 2,
                SceneObjectKind::Zone => 3,
                SceneObjectKind::ScoreSheet => 4,
            },
        );
        pose(&mut hash, object.pose);
        extents(
            &mut hash,
            object.half_extents.x,
            object.half_extents.y,
            object.half_extents.z,
        );
    }

    len_field(&mut hash, scene.cards.len());
    for card in &scene.cards {
        u64_field(&mut hash, card.id.projection_epoch);
        u8_field(&mut hash, card.id.ordinal);
        card_location(&mut hash, card.location);
        pose(&mut hash, card.pose);
        extents(
            &mut hash,
            card.half_extents.x,
            card.half_extents.y,
            card.half_extents.z,
        );
        match card.face {
            Some(face) => {
                u8_field(&mut hash, 1);
                u8_field(&mut hash, face.code());
            }
            None => u8_field(&mut hash, 0),
        }
    }

    len_field(&mut hash, scene.text.len());
    for run in &scene.text {
        u16_field(&mut hash, run.id.0);
        text_binding(&mut hash, run.binding);
        surface(&mut hash, run.attached_to);
        pose(&mut hash, run.local_pose);
        bytes(&mut hash, run.text.as_bytes());
    }
    Ok(*hash.finalize().as_bytes())
}

/// Lowercase hexadecimal form of [`spatial_scene_hash`].
///
/// # Errors
///
/// Returns the scene's structural/privacy validation failure before hashing.
pub fn spatial_scene_hash_hex(scene: &SpatialScene) -> Result<String, SceneError> {
    let mut output = String::with_capacity(64);
    for byte in spatial_scene_hash(scene)? {
        let _ = write!(output, "{byte:02x}");
    }
    Ok(output)
}

fn bytes(hash: &mut blake3::Hasher, value: &[u8]) {
    u64_field(hash, u64::try_from(value.len()).unwrap_or(u64::MAX));
    hash.update(value);
}

fn len_field(hash: &mut blake3::Hasher, value: usize) {
    u64_field(hash, u64::try_from(value).unwrap_or(u64::MAX));
}

fn u8_field(hash: &mut blake3::Hasher, value: u8) {
    hash.update(&[value]);
}

fn u16_field(hash: &mut blake3::Hasher, value: u16) {
    hash.update(&value.to_be_bytes());
}

fn u32_field(hash: &mut blake3::Hasher, value: u32) {
    hash.update(&value.to_be_bytes());
}

fn u64_field(hash: &mut blake3::Hasher, value: u64) {
    hash.update(&value.to_be_bytes());
}

fn i32_field(hash: &mut blake3::Hasher, value: i32) {
    hash.update(&value.to_be_bytes());
}

fn point(hash: &mut blake3::Hasher, value: Point3Mm) {
    i32_field(hash, value.x.get());
    i32_field(hash, value.y.get());
    i32_field(hash, value.z.get());
}

fn pose(hash: &mut blake3::Hasher, value: PoseMm) {
    point(hash, value.translation);
    u32_field(hash, value.yaw.get());
}

fn extents(hash: &mut blake3::Hasher, x: u32, y: u32, z: u32) {
    u32_field(hash, x);
    u32_field(hash, y);
    u32_field(hash, z);
}

fn seat(hash: &mut blake3::Hasher, seat: crate::SeatId) {
    u8_field(hash, seat.get());
}

fn zone(hash: &mut blake3::Hasher, zone: ZoneId) {
    match zone {
        ZoneId::Deck => u8_field(hash, 0),
        ZoneId::Trump => u8_field(hash, 1),
        ZoneId::Hand(owner) => {
            u8_field(hash, 2);
            seat(hash, owner);
        }
        ZoneId::Play => u8_field(hash, 3),
        ZoneId::Won(owner) => {
            u8_field(hash, 4);
            seat(hash, owner);
        }
    }
}

fn object_id(hash: &mut blake3::Hasher, id: ObjectId) {
    match id {
        ObjectId::Table => u8_field(hash, 0),
        ObjectId::Seat(owner) => {
            u8_field(hash, 1);
            seat(hash, owner);
        }
        ObjectId::Player(owner) => {
            u8_field(hash, 2);
            seat(hash, owner);
        }
        ObjectId::Zone(value) => {
            u8_field(hash, 3);
            zone(hash, value);
        }
        ObjectId::ScoreSheet => u8_field(hash, 4),
        ObjectId::Card(card) => {
            u8_field(hash, 5);
            u64_field(hash, card.projection_epoch);
            u8_field(hash, card.ordinal);
        }
    }
}

fn card_location(hash: &mut blake3::Hasher, value: CardLocation) {
    match value {
        CardLocation::Deck { index_from_bottom } => {
            u8_field(hash, 0);
            u8_field(hash, index_from_bottom);
        }
        CardLocation::Trump => u8_field(hash, 1),
        CardLocation::Hand {
            seat: owner,
            index_from_left,
        } => {
            u8_field(hash, 2);
            seat(hash, owner);
            u8_field(hash, index_from_left);
        }
        CardLocation::Play { seat: owner } => {
            u8_field(hash, 3);
            seat(hash, owner);
        }
        CardLocation::Won {
            seat: owner,
            trick,
            index,
        } => {
            u8_field(hash, 4);
            seat(hash, owner);
            u8_field(hash, trick);
            u8_field(hash, index);
        }
    }
}

fn surface(hash: &mut blake3::Hasher, value: SurfaceId) {
    object_id(hash, value.object);
    u8_field(
        hash,
        match value.kind {
            SurfaceKind::Face => 0,
            SurfaceKind::Back => 1,
            SurfaceKind::Top => 2,
        },
    );
}

fn text_binding(hash: &mut blake3::Hasher, value: TextBinding) {
    match value {
        TextBinding::CardFace(card) => {
            u8_field(hash, 0);
            u64_field(hash, card.projection_epoch);
            u8_field(hash, card.ordinal);
        }
        TextBinding::PlayerName(owner) => {
            u8_field(hash, 1);
            seat(hash, owner);
        }
        TextBinding::PlayerScore(owner) => {
            u8_field(hash, 2);
            seat(hash, owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        LayoutId, PlayerSpatialProjection, SeatId, TableId, ViewerSpatialProjection,
        realize_viewer_scene, registered_layout,
    };

    use super::{spatial_scene_hash, spatial_scene_hash_hex};

    #[test]
    fn canonical_hash_is_repeatable_and_sensitive_to_projection_epoch() {
        let layout = registered_layout(TableId::new(7), LayoutId::new(2, 1).expect("layout ID"))
            .expect("layout");
        let projection = ViewerSpatialProjection {
            projection_epoch: 9,
            players: (0..2)
                .map(|ordinal| PlayerSpatialProjection {
                    seat: SeatId::new(ordinal, layout.id()).expect("seat"),
                    display_name: format!("player-{ordinal}"),
                    score: u16::from(ordinal),
                    hand_count: 0,
                    visible_hand: None,
                    tricks_won: 0,
                })
                .collect(),
            trump: None,
            current_trick: Vec::new(),
            revealed_won_cards: Vec::new(),
        };
        let first = realize_viewer_scene(&layout, &projection).expect("scene");
        let second = realize_viewer_scene(&layout, &projection).expect("same scene");
        assert_eq!(spatial_scene_hash(&first), spatial_scene_hash(&second));
        assert_eq!(spatial_scene_hash_hex(&first).expect("hex").len(), 64);

        let mut changed = projection;
        changed.projection_epoch = 10;
        let changed = realize_viewer_scene(&layout, &changed).expect("changed scene");
        assert_ne!(spatial_scene_hash(&first), spatial_scene_hash(&changed));
    }
}

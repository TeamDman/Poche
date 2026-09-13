// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SpacetimeDB authority for the Poche desktop vertical slice.
//!
//! Logical room membership and seats are durable, card faces are private, and
//! physical card poses are public latest-value rows.  Renderers remain clients
//! of this model; no Bevy type crosses this boundary.

use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext};

const MAX_NAME_LEN: usize = 32;
const MAX_POSE_MM: i32 = 10_000;
const MAX_ROTATION_MDEG: i32 = 360_000;
const CARDS_PER_PLAYER: u8 = 5;

#[spacetimedb::table(accessor = room)]
pub struct Room {
    #[primary_key]
    pub room_id: String,
    pub coordinator: Identity,
    pub created_at: Timestamp,
}

/// Join capabilities are deliberately absent from public subscriptions.
#[spacetimedb::table(accessor = room_secret)]
pub struct RoomSecret {
    #[primary_key]
    pub room_id: String,
    #[unique]
    pub join_code: String,
}

#[spacetimedb::table(accessor = member)]
pub struct Member {
    #[primary_key]
    pub member_key: String,
    #[index(btree)]
    pub room_id: String,
    #[index(btree)]
    pub identity: Identity,
    pub display_name: String,
    pub seat: Option<u8>,
    pub connected: bool,
}

/// Card ownership and face are never exposed as a public table.
#[spacetimedb::table(accessor = private_hand_card)]
pub struct PrivateHandCard {
    #[primary_key]
    pub card_key: String,
    #[index(btree)]
    pub room_id: String,
    #[index(btree)]
    pub owner: Identity,
    pub card_id: String,
    pub face: String,
}

/// Rooms are visible only after the caller has joined them.
#[spacetimedb::view(accessor = my_rooms, public, primary_key = room_id)]
pub fn my_rooms(ctx: &ViewContext) -> Vec<Room> {
    ctx.db
        .member()
        .identity()
        .filter(&ctx.sender())
        .filter_map(|membership| ctx.db.room().room_id().find(&membership.room_id))
        .collect()
}

/// A member can see the membership roster for each room they have joined.
#[spacetimedb::view(accessor = room_members, public, primary_key = member_key)]
pub fn room_members(ctx: &ViewContext) -> Vec<Member> {
    let memberships: Vec<_> = ctx.db.member().identity().filter(&ctx.sender()).collect();
    memberships
        .into_iter()
        .flat_map(|membership| {
            ctx.db
                .member()
                .room_id()
                .filter(&membership.room_id)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Each caller sees only its own card faces.
#[spacetimedb::view(accessor = my_hand, public, primary_key = card_key)]
pub fn my_hand(ctx: &ViewContext) -> Vec<PrivateHandCard> {
    ctx.db
        .private_hand_card()
        .owner()
        .filter(&ctx.sender())
        .collect()
}

/// A public, face-free physical realization of a private card.
#[spacetimedb::table(accessor = card_pose)]
pub struct CardPose {
    #[primary_key]
    pub card_key: String,
    #[index(btree)]
    pub room_id: String,
    pub card_id: String,
    pub owner: Identity,
    pub owner_seat: u8,
    pub logical_location: String,
    pub x_mm: i32,
    pub y_mm: i32,
    pub z_mm: i32,
    pub rx_mdeg: i32,
    pub ry_mdeg: i32,
    pub rz_mdeg: i32,
    pub sequence: u64,
    pub committed_at: Timestamp,
}

/// Face-free poses are shared with members of the same room, not globally.
#[spacetimedb::view(accessor = visible_card_poses, public, primary_key = card_key)]
pub fn visible_card_poses(ctx: &ViewContext) -> Vec<CardPose> {
    let memberships: Vec<_> = ctx.db.member().identity().filter(&ctx.sender()).collect();
    memberships
        .into_iter()
        .flat_map(|membership| {
            ctx.db
                .card_pose()
                .room_id()
                .filter(&membership.room_id)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn create_room(
    ctx: &ReducerContext,
    room_id: String,
    join_code: String,
    display_name: String,
) -> Result<(), String> {
    validate_identifier("room id", &room_id, 64)?;
    validate_identifier("join code", &join_code, 64)?;
    validate_name(&display_name)?;
    if ctx.db.room().room_id().find(&room_id).is_some() {
        return Err("room already exists".into());
    }
    if ctx.db.room_secret().join_code().find(&join_code).is_some() {
        return Err("join code already exists".into());
    }

    ctx.db.room().insert(Room {
        room_id: room_id.clone(),
        coordinator: ctx.sender(),
        created_at: ctx.timestamp,
    });
    ctx.db.room_secret().insert(RoomSecret {
        room_id: room_id.clone(),
        join_code,
    });
    insert_member(ctx, room_id, display_name);
    Ok(())
}

#[spacetimedb::reducer]
pub fn join_room(
    ctx: &ReducerContext,
    join_code: String,
    display_name: String,
) -> Result<(), String> {
    validate_name(&display_name)?;
    let secret = ctx
        .db
        .room_secret()
        .join_code()
        .find(&join_code)
        .ok_or_else(|| "unknown or expired room code".to_string())?;
    let member_key = member_key(&secret.room_id, ctx.sender());
    if let Some(mut member) = ctx.db.member().member_key().find(&member_key) {
        member.display_name = display_name;
        member.connected = true;
        ctx.db.member().member_key().update(member);
        return Ok(());
    }
    insert_member(ctx, secret.room_id, display_name);
    Ok(())
}

#[spacetimedb::reducer]
pub fn take_seat(ctx: &ReducerContext, room_id: String, seat: u8) -> Result<(), String> {
    if seat > 1 {
        return Err("the first desktop slice supports seats 0 and 1".into());
    }
    let key = member_key(&room_id, ctx.sender());
    let mut caller = ctx
        .db
        .member()
        .member_key()
        .find(&key)
        .ok_or_else(|| "join the room before taking a seat".to_string())?;
    let occupied = ctx
        .db
        .member()
        .room_id()
        .filter(&room_id)
        .any(|candidate| candidate.seat == Some(seat) && candidate.identity != ctx.sender());
    if occupied {
        return Err(format!("seat {seat} is already occupied"));
    }
    caller.seat = Some(seat);
    caller.connected = true;
    ctx.db.member().member_key().update(caller);
    ensure_deal(ctx, &room_id);
    Ok(())
}

#[spacetimedb::reducer]
pub fn release_seat(ctx: &ReducerContext, room_id: String) -> Result<(), String> {
    let key = member_key(&room_id, ctx.sender());
    let mut caller = ctx
        .db
        .member()
        .member_key()
        .find(&key)
        .ok_or_else(|| "not a room member".to_string())?;
    caller.seat = None;
    caller.connected = true;
    ctx.db.member().member_key().update(caller);
    Ok(())
}

#[spacetimedb::reducer]
#[allow(clippy::too_many_arguments)]
pub fn set_card_pose(
    ctx: &ReducerContext,
    room_id: String,
    card_id: String,
    sequence: u64,
    x_mm: i32,
    y_mm: i32,
    z_mm: i32,
    rx_mdeg: i32,
    ry_mdeg: i32,
    rz_mdeg: i32,
) -> Result<(), String> {
    validate_pose(x_mm, y_mm, z_mm, rx_mdeg, ry_mdeg, rz_mdeg)?;
    let card_key = card_key(&room_id, ctx.sender(), &card_id);
    let private_card = ctx
        .db
        .private_hand_card()
        .card_key()
        .find(&card_key)
        .ok_or_else(|| "card is not controlled by this identity".to_string())?;
    if private_card.owner != ctx.sender() {
        return Err("card is not controlled by this identity".into());
    }
    let mut pose = ctx
        .db
        .card_pose()
        .card_key()
        .find(&card_key)
        .ok_or_else(|| "card pose is missing".to_string())?;
    if sequence <= pose.sequence {
        return Err("stale card pose sequence".into());
    }
    pose.x_mm = x_mm;
    pose.y_mm = y_mm;
    pose.z_mm = z_mm;
    pose.rx_mdeg = rx_mdeg;
    pose.ry_mdeg = ry_mdeg;
    pose.rz_mdeg = rz_mdeg;
    pose.sequence = sequence;
    pose.committed_at = ctx.timestamp;
    ctx.db.card_pose().card_key().update(pose);
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: String) -> Result<(), String> {
    let key = member_key(&room_id, ctx.sender());
    if !ctx.db.member().member_key().delete(&key) {
        return Err("not a room member".into());
    }
    delete_identity_cards(ctx, &room_id, ctx.sender());
    if ctx.db.member().room_id().filter(&room_id).next().is_none() {
        ctx.db.room().room_id().delete(&room_id);
        ctx.db.room_secret().room_id().delete(&room_id);
    }
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) {
    set_connected(ctx, true);
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    // Connection state is advisory. Durable membership and seats survive so a
    // token-preserving reconnect can resume without impersonation.
    set_connected(ctx, false);
}

fn insert_member(ctx: &ReducerContext, room_id: String, display_name: String) {
    ctx.db.member().insert(Member {
        member_key: member_key(&room_id, ctx.sender()),
        room_id,
        identity: ctx.sender(),
        display_name,
        seat: None,
        connected: true,
    });
}

fn ensure_deal(ctx: &ReducerContext, room_id: &str) {
    let seated: Vec<_> = ctx
        .db
        .member()
        .room_id()
        .filter(room_id)
        .filter(|member| member.seat.is_some())
        .collect();
    if seated.len() != 2 {
        return;
    }
    for member in seated {
        let seat = member.seat.expect("filtered to seated members");
        for slot in 0..CARDS_PER_PLAYER {
            let card_id = format!("card-{seat}-{slot}");
            let key = card_key(room_id, member.identity, &card_id);
            if ctx.db.private_hand_card().card_key().find(&key).is_some() {
                continue;
            }
            let face = card_face(seat, slot).to_string();
            ctx.db.private_hand_card().insert(PrivateHandCard {
                card_key: key.clone(),
                room_id: room_id.to_string(),
                owner: member.identity,
                card_id: card_id.clone(),
                face,
            });
            let base_x = if seat == 0 { -1_800 } else { 1_800 };
            let base_z = if seat == 0 { 1_200 } else { -1_200 };
            ctx.db.card_pose().insert(CardPose {
                card_key: key,
                room_id: room_id.to_string(),
                card_id,
                owner: member.identity,
                owner_seat: seat,
                logical_location: "hand".into(),
                x_mm: base_x + i32::from(slot) * 180,
                y_mm: 80,
                z_mm: base_z,
                rx_mdeg: 0,
                ry_mdeg: if seat == 0 { 0 } else { 180_000 },
                rz_mdeg: 0,
                sequence: 0,
                committed_at: ctx.timestamp,
            });
        }
    }
}

fn delete_identity_cards(ctx: &ReducerContext, room_id: &str, identity: Identity) {
    let keys: Vec<_> = ctx
        .db
        .private_hand_card()
        .owner()
        .filter(&identity)
        .filter(|card| card.room_id == room_id)
        .map(|card| card.card_key)
        .collect();
    for key in keys {
        ctx.db.private_hand_card().card_key().delete(&key);
        ctx.db.card_pose().card_key().delete(&key);
    }
}

fn set_connected(ctx: &ReducerContext, connected: bool) {
    let members: Vec<_> = ctx.db.member().identity().filter(&ctx.sender()).collect();
    for mut member in members {
        member.connected = connected;
        ctx.db.member().member_key().update(member);
    }
}

fn member_key(room_id: &str, identity: Identity) -> String {
    format!("{room_id}:{}", identity.to_hex())
}

fn card_key(room_id: &str, identity: Identity, card_id: &str) -> String {
    format!("{room_id}:{}:{card_id}", identity.to_hex())
}

fn validate_identifier(label: &str, value: &str, max_len: usize) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(format!("{label} has an invalid shape"));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LEN || name.chars().any(char::is_control)
    {
        return Err(format!(
            "display name must contain 1 to {MAX_NAME_LEN} visible characters"
        ));
    }
    Ok(())
}

fn validate_pose(
    x_mm: i32,
    y_mm: i32,
    z_mm: i32,
    rx_mdeg: i32,
    ry_mdeg: i32,
    rz_mdeg: i32,
) -> Result<(), String> {
    if [x_mm, y_mm, z_mm]
        .into_iter()
        .any(|component| component.abs() > MAX_POSE_MM)
    {
        return Err("card position is outside the tabletop envelope".into());
    }
    if [rx_mdeg, ry_mdeg, rz_mdeg]
        .into_iter()
        .any(|component| component.abs() > MAX_ROTATION_MDEG)
    {
        return Err("card rotation is outside the normalized envelope".into());
    }
    Ok(())
}

fn card_face(seat: u8, slot: u8) -> &'static str {
    const FACES: [[&str; CARDS_PER_PLAYER as usize]; 2] = [
        ["A♣", "3♦", "5♥", "7♠", "9♣"],
        ["2♣", "4♦", "6♥", "8♠", "10♣"],
    ];
    FACES[usize::from(seat)][usize::from(slot)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_face_sets_are_distinct() {
        let alice: Vec<_> = (0..CARDS_PER_PLAYER)
            .map(|slot| card_face(0, slot))
            .collect();
        let bob: Vec<_> = (0..CARDS_PER_PLAYER)
            .map(|slot| card_face(1, slot))
            .collect();
        assert!(alice.iter().all(|face| !bob.contains(face)));
    }

    #[test]
    fn identifiers_reject_query_punctuation() {
        assert!(validate_identifier("room id", "room-AB12", 64).is_ok());
        assert!(validate_identifier("room id", "room?scan=*", 64).is_err());
    }

    #[test]
    fn pose_bounds_are_explicit() {
        assert!(validate_pose(10_000, 0, -10_000, 360_000, 0, -360_000).is_ok());
        assert!(validate_pose(10_001, 0, 0, 0, 0, 0).is_err());
    }
}

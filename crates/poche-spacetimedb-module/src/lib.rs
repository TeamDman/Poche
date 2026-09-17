// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `SpacetimeDB` authority for the Poche desktop vertical slice.
//!
//! Logical room membership and seats are durable, card faces are private, and
//! physical card poses are public latest-value rows.  Renderers remain clients
//! of this model; no Bevy type crosses this boundary.

#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    reason = "SpacetimeDB exports fixed owned reducer/view signatures and pose axes use the established rx/ry/rz schema"
)]

use poche_environment::{
    EnvironmentAction, GameEnvironment, OracleChanceAction, OracleEnvironment, OraclePlayerAction,
};
use poche_oracle_rust::{Card, Game, PhaseTag, Seat, Turn};
use spacetimedb::{ConnectionId, Identity, ReducerContext, Table, Timestamp, ViewContext};
mod money;
mod round_flow;

const MAX_NAME_LEN: usize = 32;
const MAX_POSE_MM: i32 = 10_000;
const MAX_ROTATION_MDEG: i32 = 360_000;
const PLAYERS: usize = 2;
const LAYOUT_HAND_Z_MM: i32 = 520;
const LAYOUT_PLAY_Z_MM: i32 = 50;
const LAYOUT_WON_Z_MM: i32 = 300;

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

/// The one room projected to a device identity at a time.
///
/// Membership remains durable so reconnecting can resume a player, but the
/// public views must never union every historical room attached to that
/// identity. Creating or joining a room moves this focus atomically.
#[spacetimedb::table(accessor = active_room)]
pub struct ActiveRoom {
    #[primary_key]
    pub identity: Identity,
    #[index(btree)]
    pub room_id: String,
}

/// One ephemeral row per live SDK connection.
///
/// Several rows may belong to one application identity. Durable membership is
/// online while any such row remains; disconnecting one window must not mark a
/// second window using the same protected identity offline.
#[spacetimedb::table(accessor = connection_presence)]
pub struct ConnectionPresence {
    #[primary_key]
    pub connection_id: ConnectionId,
    #[index(btree)]
    pub identity: Identity,
    pub connected_at: Timestamp,
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

/// Private mapping from an opaque physical object to the rules engine's card.
#[spacetimedb::table(accessor = private_card_identity)]
pub struct PrivateCardIdentity {
    #[primary_key]
    pub card_key: String,
    #[index(btree)]
    pub room_id: String,
    #[index(btree)]
    pub owner: Identity,
    pub card_id: String,
    pub face_code: u8,
}

/// Replay root and viewer-safe latest game projection for one room.
#[spacetimedb::table(accessor = room_game)]
pub struct RoomGame {
    #[primary_key]
    pub room_id: String,
    pub seed: u64,
    pub action_count: u64,
    pub phase: String,
    pub actor_seat: Option<u8>,
    pub dealer_seat: Option<u8>,
    pub round_index: u16,
    pub hand_size: u8,
    pub hand_count_0: u8,
    pub hand_count_1: u8,
    pub bid_0: Option<u8>,
    pub bid_1: Option<u8>,
    pub trick_count: u8,
    pub trick_seat_0: Option<u8>,
    pub trick_card_0: Option<u8>,
    pub trick_seat_1: Option<u8>,
    pub trick_card_1: Option<u8>,
    pub tricks_won_0: u8,
    pub tricks_won_1: u8,
    pub score_0: u16,
    pub score_1: u16,
    pub pot_cents: u32,
    pub trump: Option<u8>,
}

/// One ordered player action. Chance is reconstructed from [`RoomGame::seed`].
#[spacetimedb::table(accessor = game_action)]
pub struct GameAction {
    #[primary_key]
    pub action_key: String,
    #[index(btree)]
    pub room_id: String,
    pub sequence: u64,
    pub seat: u8,
    pub kind: String,
    pub value: u8,
}

/// Immutable public score-sheet row. Payments are obligations, not minted money.
#[spacetimedb::table(accessor = round_record)]
pub struct RoundRecord {
    #[primary_key]
    pub round_key: String,
    #[index(btree)]
    pub room_id: String,
    pub round_index: u16,
    pub dealer_seat: u8,
    pub hand_size: u8,
    pub bids: Vec<u8>,
    pub tricks_won: Vec<u8>,
    pub points: Vec<u16>,
    pub totals: Vec<u16>,
    pub payment_cents: Vec<u32>,
}

#[spacetimedb::view(accessor = visible_rounds, public, primary_key = round_key)]
pub fn visible_rounds(ctx: &ViewContext) -> Vec<RoundRecord> {
    active_room_id(ctx).map_or_else(Vec::new, |room| {
        ctx.db.round_record().room_id().filter(&room).collect()
    })
}

/// An append-only, viewer-safe account of accepted room activity.
///
/// Summaries are constructed by reducers exclusively from information which is
/// already public at that point in the game. In particular, an unplayed card
/// face is never written here.
#[spacetimedb::table(accessor = activity_event)]
pub struct ActivityEvent {
    #[primary_key]
    pub event_key: String,
    #[index(btree)]
    pub room_id: String,
    pub sequence: u64,
    pub kind: String,
    pub summary: String,
    pub occurred_at: Timestamp,
}

/// A face becomes public only after the pure rules engine accepts its play.
#[spacetimedb::table(accessor = revealed_card)]
pub struct RevealedCard {
    #[primary_key]
    pub card_key: String,
    #[index(btree)]
    pub room_id: String,
    pub face: String,
}

/// Rooms are visible only after the caller has joined them.
#[spacetimedb::view(accessor = my_rooms, public, primary_key = room_id)]
pub fn my_rooms(ctx: &ViewContext) -> Vec<Room> {
    active_room_id(ctx)
        .and_then(|room_id| ctx.db.room().room_id().find(&room_id))
        .into_iter()
        .collect()
}

/// A member can recover the bearer capability for only its active room.
///
/// The underlying secret table remains private. This sender-scoped view lets
/// another authenticated device for the same member resume and invite peers
/// without making capabilities visible to non-members.
#[spacetimedb::view(accessor = my_room_capability, public, primary_key = room_id)]
pub fn my_room_capability(ctx: &ViewContext) -> Vec<RoomSecret> {
    active_room_id(ctx)
        .and_then(|room_id| ctx.db.room_secret().room_id().find(&room_id))
        .into_iter()
        .collect()
}

/// A member can see the membership roster for each room they have joined.
#[spacetimedb::view(accessor = room_members, public, primary_key = member_key)]
pub fn room_members(ctx: &ViewContext) -> Vec<Member> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db.member().room_id().filter(&room_id).collect()
    })
}

/// Each caller sees only its own card faces.
#[spacetimedb::view(accessor = my_hand, public, primary_key = card_key)]
pub fn my_hand(ctx: &ViewContext) -> Vec<PrivateHandCard> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db
            .private_hand_card()
            .owner()
            .filter(&ctx.sender())
            .filter(|card| card.room_id == room_id)
            .collect()
    })
}

/// Members receive the latest public game projection for their rooms.
#[spacetimedb::view(accessor = visible_room_games, public, primary_key = room_id)]
pub fn visible_room_games(ctx: &ViewContext) -> Vec<RoomGame> {
    active_room_id(ctx)
        .and_then(|room_id| ctx.db.room_game().room_id().find(&room_id))
        .into_iter()
        .collect()
}

/// Members receive only card faces already revealed by accepted play.
#[spacetimedb::view(accessor = visible_revealed_cards, public, primary_key = card_key)]
pub fn visible_revealed_cards(ctx: &ViewContext) -> Vec<RevealedCard> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db.revealed_card().room_id().filter(&room_id).collect()
    })
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

/// Actual conserved coin objects; the sender-scoped view is the only public
/// read path. Leaving membership does not discard or recreate an inventory.
#[spacetimedb::table(accessor = coin)]
pub struct Coin {
    #[primary_key]
    pub coin_key: String,
    #[index(btree)]
    pub room_id: String,
    #[index(btree)]
    pub owner: Identity,
    pub coin_id: String,
    pub denomination_cents: u8,
    pub container: String,
    pub x_mm: i32,
    pub y_mm: i32,
    pub z_mm: i32,
    pub sequence: u64,
    pub committed_at: Timestamp,
}

#[spacetimedb::view(accessor = visible_coins, public, primary_key = coin_key)]
pub fn visible_coins(ctx: &ViewContext) -> Vec<Coin> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db.coin().room_id().filter(&room_id).collect()
    })
}

/// Preview a coin pose or commit a transfer. A true commit snaps the coin into
/// its canonical destination pile; the server never trusts a client balance.
#[spacetimedb::reducer]
#[allow(clippy::too_many_arguments)]
pub fn move_coin(
    ctx: &ReducerContext,
    room_id: String,
    coin_id: String,
    sequence: u64,
    container: String,
    x_mm: i32,
    y_mm: i32,
    z_mm: i32,
    commit: bool,
) -> Result<(), String> {
    money::move_coin(
        ctx,
        &room_id,
        &coin_id,
        sequence,
        &container,
        [x_mm, y_mm, z_mm],
        commit,
    )
}

/// Face-free poses are shared with members of the same room, not globally.
#[spacetimedb::view(accessor = visible_card_poses, public, primary_key = card_key)]
pub fn visible_card_poses(ctx: &ViewContext) -> Vec<CardPose> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db.card_pose().room_id().filter(&room_id).collect()
    })
}

/// Members receive an ordered history containing public facts only.
#[spacetimedb::view(accessor = visible_activity, public, primary_key = event_key)]
pub fn visible_activity(ctx: &ViewContext) -> Vec<ActivityEvent> {
    active_room_id(ctx).map_or_else(Vec::new, |room_id| {
        ctx.db.activity_event().room_id().filter(&room_id).collect()
    })
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
    insert_member(ctx, room_id.clone(), display_name.clone());
    activate_room(ctx, room_id.clone());
    append_activity(
        ctx,
        &room_id,
        "room-created",
        format!("{display_name} created the lobby"),
    );
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
        let prior_name = member.display_name.clone();
        member.display_name.clone_from(&display_name);
        member.connected = identity_has_connections(ctx, ctx.sender());
        ctx.db.member().member_key().update(member);
        activate_room(ctx, secret.room_id.clone());
        append_activity(
            ctx,
            &secret.room_id,
            "room-rejoined",
            format!("{prior_name} rejoined the lobby as {display_name}"),
        );
        money::migrate_legacy_room(ctx, &secret.room_id);
        repair_deal_if_incoherent(ctx, &secret.room_id);
        settle_if_paid(ctx, &secret.room_id)?;
        return Ok(());
    }
    insert_member(ctx, secret.room_id.clone(), display_name.clone());
    activate_room(ctx, secret.room_id.clone());
    append_activity(
        ctx,
        &secret.room_id,
        "room-joined",
        format!("{display_name} joined the lobby"),
    );
    money::migrate_legacy_room(ctx, &secret.room_id);
    repair_deal_if_incoherent(ctx, &secret.room_id);
    settle_if_paid(ctx, &secret.room_id)?;
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
    money::migrate_legacy_room(ctx, &room_id);
    let display_name = caller.display_name.clone();
    caller.seat = Some(seat);
    caller.connected = identity_has_connections(ctx, ctx.sender());
    ctx.db.member().member_key().update(caller);
    money::ensure_inventory(ctx, &room_id, ctx.sender(), seat);
    money::place_inventory(ctx, &room_id, ctx.sender(), seat);
    append_activity(
        ctx,
        &room_id,
        "seat-taken",
        format!("{display_name} took seat {}", seat + 1),
    );
    repair_deal_if_incoherent(ctx, &room_id);
    ensure_deal(ctx, &room_id)?;
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
    let display_name = caller.display_name.clone();
    let left_seat = caller.seat.take();
    caller.connected = identity_has_connections(ctx, ctx.sender());
    ctx.db.member().member_key().update(caller);
    if let Some(seat) = left_seat {
        append_activity(
            ctx,
            &room_id,
            "seat-released",
            format!("{display_name} left seat {}", seat + 1),
        );
        if abandon_deal(ctx, &room_id) {
            append_activity(
                ctx,
                &room_id,
                "deal-abandoned",
                "The active deal was abandoned because a seat became empty".into(),
            );
        } else {
            money::refund_owner(ctx, &room_id, ctx.sender(), seat);
        }
    }
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
    let seat = caller_seat(ctx, &room_id)?;
    let mut pose = ctx
        .db
        .card_pose()
        .room_id()
        .filter(&room_id)
        .find(|pose| pose.card_id == card_id)
        .ok_or_else(|| "card pose is missing".to_string())?;
    if !((pose.logical_location == "hand" && pose.owner == ctx.sender())
        || pose.logical_location == format!("won:{seat}"))
    {
        return Err("only your hand or your won trick pile may be manipulated".into());
    }
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

/// After all missed-bid payments, only the next dealer may collect/shuffle/deal.
#[spacetimedb::reducer]
pub fn deal_next_round(ctx: &ReducerContext, room_id: String) -> Result<(), String> {
    let seat = caller_seat(ctx, &room_id)?;
    settle_if_paid(ctx, &room_id)?;
    let mut record = ctx
        .db
        .room_game()
        .room_id()
        .find(&room_id)
        .ok_or("pay both opening antes before the first deal")?;
    if record.phase != "awaiting-deal" {
        return Err("finish the round and pay every missed-bid dime before dealing".into());
    }
    if record.dealer_seat != Some(seat) {
        return Err("only the next dealer may deal this round".into());
    }
    let game = reconstruct_game(ctx, &record)?;
    let next = round_flow::deal(&game, record.seed)?;
    append_action(ctx, &record, seat, "deal", 0);
    record.action_count += 1;
    write_room_game(ctx, record, &next)?;
    clear_round_cards(ctx, &room_id);
    materialize_hands(ctx, &room_id, &next)?;
    append_activity(
        ctx,
        &room_id,
        "deal-started",
        format!(
            "{} shuffled and dealt round {}",
            caller_name(ctx, &room_id)?,
            next.observe(Seat::new(0).map_err(rule_error)?).round_index + 1
        ),
    );
    Ok(())
}

/// Submit a fixed bid through the full pure Poche rules engine.
#[spacetimedb::reducer]
pub fn bid(ctx: &ReducerContext, room_id: String, tricks: u8) -> Result<(), String> {
    let seat = caller_seat(ctx, &room_id)?;
    money::migrate_legacy_room(ctx, &room_id);
    let mut record = ctx
        .db
        .room_game()
        .room_id()
        .find(&room_id)
        .ok_or_else(|| "the room does not have an active deal".to_string())?;
    let game = reconstruct_game(ctx, &record)?;
    let player = Seat::new(usize::from(seat)).map_err(rule_error)?;
    let next = transition_game(
        &game,
        EnvironmentAction::Player(OraclePlayerAction::Bid { player, tricks }),
    )?;
    append_action(ctx, &record, seat, "bid", tricks);
    append_activity(
        ctx,
        &room_id,
        "bid",
        format!(
            "{} bid {tricks} {}",
            caller_name(ctx, &room_id)?,
            if tricks == 1 { "trick" } else { "tricks" }
        ),
    );
    record.action_count = record.action_count.saturating_add(1);
    write_room_game(ctx, record, &next)?;
    Ok(())
}

/// Play one owned card. Physical placement proposes this typed transition; it
/// cannot bypass actor, ownership, phase, or follow-suit validation.
#[spacetimedb::reducer]
pub fn play_card(ctx: &ReducerContext, room_id: String, card_id: String) -> Result<(), String> {
    let seat = caller_seat(ctx, &room_id)?;
    money::migrate_legacy_room(ctx, &room_id);
    let key = card_key(&room_id, ctx.sender(), &card_id);
    let private = ctx
        .db
        .private_card_identity()
        .card_key()
        .find(&key)
        .ok_or_else(|| "card is not in this player's private hand".to_string())?;
    let mut pose = ctx
        .db
        .card_pose()
        .card_key()
        .find(&key)
        .ok_or_else(|| "card pose is missing".to_string())?;
    if private.owner != ctx.sender() || pose.logical_location != "hand" {
        return Err("card is not in this player's private hand".into());
    }

    let mut record = ctx
        .db
        .room_game()
        .room_id()
        .find(&room_id)
        .ok_or_else(|| "the room does not have an active deal".to_string())?;
    let game = reconstruct_game(ctx, &record)?;
    let player = Seat::new(usize::from(seat)).map_err(rule_error)?;
    let card = card_from_code(private.face_code)?;
    let play_index = game
        .observe(Seat::new(0).map_err(rule_error)?)
        .current_trick
        .len();
    let next = transition_game(
        &game,
        EnvironmentAction::Player(OraclePlayerAction::Play { player, card }),
    )?;

    append_action(ctx, &record, seat, "play", private.face_code);
    append_activity(
        ctx,
        &room_id,
        "card-played",
        format!(
            "{} played {}",
            caller_name(ctx, &room_id)?,
            card_label(private.face_code)?
        ),
    );
    record.action_count = record.action_count.saturating_add(1);
    write_room_game(ctx, record, &next)?;

    pose.logical_location = format!("play:{seat}");
    pose.x_mm = 0;
    pose.y_mm = 40;
    pose.z_mm = if seat == 0 {
        LAYOUT_PLAY_Z_MM
    } else {
        -LAYOUT_PLAY_Z_MM
    };
    pose.ry_mdeg = 0;
    pose.sequence = pose.sequence.saturating_add(1);
    pose.committed_at = ctx.timestamp;
    ctx.db.card_pose().card_key().update(pose);
    ctx.db.revealed_card().insert(RevealedCard {
        card_key: key.clone(),
        room_id: room_id.clone(),
        face: card_label(private.face_code)?.to_string(),
    });
    ctx.db.private_hand_card().card_key().delete(&key);
    ctx.db.private_card_identity().card_key().delete(&key);

    let observation = next.observe(Seat::new(0).map_err(rule_error)?);
    if play_index + 1 == PLAYERS {
        let previous = game.observe(Seat::new(0).map_err(rule_error)?);
        let winner = observation
            .tricks_won
            .iter()
            .zip(previous.tricks_won)
            .position(|(after, before)| *after == before + 1)
            .and_then(|index| u8::try_from(index).ok())
            .ok_or_else(|| "completed trick has no unique winner".to_string())?;
        finish_trick_poses(ctx, &room_id, winner);
    }
    settle_if_paid(ctx, &room_id)?;
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: String) -> Result<(), String> {
    let key = member_key(&room_id, ctx.sender());
    let member = ctx
        .db
        .member()
        .member_key()
        .find(&key)
        .ok_or_else(|| "not a room member".to_string())?;
    if let Some(seat) = member.seat {
        if abandon_deal(ctx, &room_id) {
            append_activity(
                ctx,
                &room_id,
                "deal-abandoned",
                "The active deal was abandoned because a seat became empty".into(),
            );
        } else {
            money::refund_owner(ctx, &room_id, ctx.sender(), seat);
        }
    }
    append_activity(
        ctx,
        &room_id,
        "room-left",
        format!("{} left the lobby", member.display_name),
    );
    if !ctx.db.member().member_key().delete(&key) {
        return Err("not a room member".into());
    }
    delete_identity_cards(ctx, &room_id, ctx.sender());
    if ctx
        .db
        .active_room()
        .identity()
        .find(&ctx.sender())
        .is_some_and(|active| active.room_id == room_id)
    {
        ctx.db.active_room().identity().delete(&ctx.sender());
    }
    if ctx.db.member().room_id().filter(&room_id).next().is_none() {
        money::delete_room_inventory(ctx, &room_id);
        ctx.db.room().room_id().delete(&room_id);
        ctx.db.room_secret().room_id().delete(&room_id);
        ctx.db.room_game().room_id().delete(&room_id);
        for action in ctx
            .db
            .game_action()
            .room_id()
            .filter(&room_id)
            .collect::<Vec<_>>()
        {
            ctx.db.game_action().action_key().delete(&action.action_key);
        }
        for card in ctx
            .db
            .revealed_card()
            .room_id()
            .filter(&room_id)
            .collect::<Vec<_>>()
        {
            ctx.db.revealed_card().card_key().delete(&card.card_key);
        }
        for event in ctx
            .db
            .activity_event()
            .room_id()
            .filter(&room_id)
            .collect::<Vec<_>>()
        {
            ctx.db.activity_event().event_key().delete(&event.event_key);
        }
    }
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) {
    let Some(connection_id) = ctx.connection_id() else {
        return;
    };
    if ctx
        .db
        .connection_presence()
        .connection_id()
        .find(connection_id)
        .is_none()
    {
        ctx.db.connection_presence().insert(ConnectionPresence {
            connection_id,
            identity: ctx.sender(),
            connected_at: ctx.timestamp,
        });
    }
    set_connected(ctx, true);
    if let Some(active) = ctx.db.active_room().identity().find(&ctx.sender()) {
        money::migrate_legacy_room(ctx, &active.room_id);
        repair_deal_if_incoherent(ctx, &active.room_id);
        if let Err(error) = settle_if_paid(ctx, &active.room_id) {
            spacetimedb::log::warn!("round recovery failed: {error}");
        }
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    // Connection state is advisory. Durable membership and seats survive so a
    // token-preserving reconnect can resume without impersonation.
    if let Some(connection_id) = ctx.connection_id() {
        ctx.db
            .connection_presence()
            .connection_id()
            .delete(connection_id);
    }
    set_connected(ctx, identity_has_connections(ctx, ctx.sender()));
}

fn insert_member(ctx: &ReducerContext, room_id: String, display_name: String) {
    ctx.db.member().insert(Member {
        member_key: member_key(&room_id, ctx.sender()),
        room_id,
        identity: ctx.sender(),
        display_name,
        seat: None,
        connected: identity_has_connections(ctx, ctx.sender()),
    });
}

fn ensure_deal(ctx: &ReducerContext, room_id: &str) -> Result<(), String> {
    let seated: Vec<_> = ctx
        .db
        .member()
        .room_id()
        .filter(room_id)
        .filter(|member| member.seat.is_some())
        .collect();
    if seated.len() != PLAYERS
        || ctx
            .db
            .room_game()
            .room_id()
            .find(&room_id.to_string())
            .is_some()
    {
        return Ok(());
    }
    if seated
        .iter()
        .any(|member| money::paid_cents(ctx, room_id, member.identity) != 25)
    {
        return Ok(());
    }
    let seed = seed_for_room(room_id);
    let game = initial_game(seed)?;
    let mut projection = project_room_game(room_id, seed, 0, &game)?;
    projection.pot_cents = money::pot_cents(ctx, room_id);
    ctx.db.room_game().insert(projection);
    append_activity(ctx, room_id, "deal-started", "The first deal began".into());
    materialize_hands(ctx, room_id, &game)
}

fn materialize_hands(
    ctx: &ReducerContext,
    room_id: &str,
    game: &Game<PLAYERS>,
) -> Result<(), String> {
    let seated = ctx
        .db
        .member()
        .room_id()
        .filter(room_id)
        .filter(|member| member.seat.is_some())
        .collect::<Vec<_>>();
    let round = game.observe(Seat::new(0).map_err(rule_error)?).round_index;
    for member in seated {
        let seat = member.seat.expect("filtered to seated members");
        let player = Seat::new(usize::from(seat)).map_err(rule_error)?;
        let hand = game.observe(player).private_hand;
        let hand_count = u8::try_from(hand.len()).map_err(|_| "hand exceeds u8".to_string())?;
        for (slot, card) in hand.iter().enumerate() {
            let slot = u8::try_from(slot).map_err(|_| "hand slot exceeds u8".to_string())?;
            let face_code = card_code(card)?;
            let card_id = if round == 0 {
                format!("card-{seat}-{slot}")
            } else {
                format!("round-{round}-card-{seat}-{slot}")
            };
            let key = card_key(room_id, member.identity, &card_id);
            if ctx.db.private_hand_card().card_key().find(&key).is_some() {
                continue;
            }
            ctx.db.private_hand_card().insert(PrivateHandCard {
                card_key: key.clone(),
                room_id: room_id.to_string(),
                owner: member.identity,
                card_id: card_id.clone(),
                face: card_label(face_code)?.to_string(),
            });
            ctx.db.private_card_identity().insert(PrivateCardIdentity {
                card_key: key.clone(),
                room_id: room_id.to_string(),
                owner: member.identity,
                card_id: card_id.clone(),
                face_code,
            });
            let [x_mm, y_mm, z_mm] = hand_position(seat, slot, hand_count);
            ctx.db.card_pose().insert(CardPose {
                card_key: key,
                room_id: room_id.to_string(),
                card_id,
                owner: member.identity,
                owner_seat: seat,
                logical_location: "hand".into(),
                x_mm,
                y_mm,
                z_mm,
                rx_mdeg: 0,
                ry_mdeg: if seat == 0 { 0 } else { 180_000 },
                rz_mdeg: 0,
                sequence: 0,
                committed_at: ctx.timestamp,
            });
        }
    }
    Ok(())
}

fn abandon_deal(ctx: &ReducerContext, room_id: &str) -> bool {
    let had_deal = ctx.db.room_game().room_id().delete(&room_id.to_string());
    if had_deal {
        money::refund_all(ctx, room_id);
    }
    for action in ctx
        .db
        .game_action()
        .room_id()
        .filter(room_id)
        .collect::<Vec<_>>()
    {
        ctx.db.game_action().action_key().delete(&action.action_key);
    }
    for row in ctx
        .db
        .round_record()
        .room_id()
        .filter(room_id)
        .collect::<Vec<_>>()
    {
        ctx.db.round_record().round_key().delete(&row.round_key);
    }
    clear_round_cards(ctx, room_id);
    had_deal
}

fn clear_round_cards(ctx: &ReducerContext, room_id: &str) {
    for card in ctx
        .db
        .card_pose()
        .room_id()
        .filter(room_id)
        .collect::<Vec<_>>()
    {
        ctx.db.private_hand_card().card_key().delete(&card.card_key);
        ctx.db
            .private_card_identity()
            .card_key()
            .delete(&card.card_key);
        ctx.db.revealed_card().card_key().delete(&card.card_key);
        ctx.db.card_pose().card_key().delete(&card.card_key);
    }
}

fn repair_deal_if_incoherent(ctx: &ReducerContext, room_id: &str) {
    let Some(game) = ctx.db.room_game().room_id().find(&room_id.to_string()) else {
        return;
    };
    let seated = ctx
        .db
        .member()
        .room_id()
        .filter(room_id)
        .filter_map(|member| member.seat.map(|seat| (seat, member.identity)))
        .collect::<Vec<_>>();
    let coherent = seated.len() == PLAYERS
        && [game.hand_count_0, game.hand_count_1]
            .into_iter()
            .enumerate()
            .all(|(seat, expected)| {
                let Some((_, owner)) = seated
                    .iter()
                    .find(|(candidate, _)| usize::from(*candidate) == seat)
                else {
                    return false;
                };
                let actual = ctx
                    .db
                    .private_hand_card()
                    .owner()
                    .filter(owner)
                    .filter(|card| card.room_id == room_id)
                    .count();
                usize::from(expected) == actual
            });
    if !coherent && abandon_deal(ctx, room_id) {
        append_activity(
            ctx,
            room_id,
            "deal-repaired",
            "The authority returned an incomplete deal to the lobby".into(),
        );
    }
}

fn append_activity(ctx: &ReducerContext, room_id: &str, kind: &str, summary: String) {
    let sequence = ctx
        .db
        .activity_event()
        .room_id()
        .filter(room_id)
        .map(|event| event.sequence)
        .max()
        .map_or(0, |value| value.saturating_add(1));
    ctx.db.activity_event().insert(ActivityEvent {
        event_key: format!("{room_id}:{sequence}"),
        room_id: room_id.to_string(),
        sequence,
        kind: kind.to_string(),
        summary,
        occurred_at: ctx.timestamp,
    });
}

fn caller_name(ctx: &ReducerContext, room_id: &str) -> Result<String, String> {
    ctx.db
        .member()
        .member_key()
        .find(&member_key(room_id, ctx.sender()))
        .map(|member| member.display_name)
        .ok_or_else(|| "join the room before acting".to_string())
}

fn delete_identity_cards(ctx: &ReducerContext, room_id: &str, identity: Identity) {
    let keys: Vec<_> = ctx
        .db
        .card_pose()
        .room_id()
        .filter(room_id)
        .filter(|card| card.owner == identity)
        .map(|card| card.card_key)
        .collect();
    for key in keys {
        ctx.db.private_hand_card().card_key().delete(&key);
        ctx.db.private_card_identity().card_key().delete(&key);
        ctx.db.revealed_card().card_key().delete(&key);
        ctx.db.card_pose().card_key().delete(&key);
    }
}

fn set_connected(ctx: &ReducerContext, connected: bool) {
    let Some(room_id) = ctx
        .db
        .active_room()
        .identity()
        .find(&ctx.sender())
        .map(|active| active.room_id)
    else {
        return;
    };
    let key = member_key(&room_id, ctx.sender());
    if let Some(mut member) = ctx.db.member().member_key().find(&key) {
        member.connected = connected;
        ctx.db.member().member_key().update(member);
    }
}

fn identity_has_connections(ctx: &ReducerContext, identity: Identity) -> bool {
    ctx.db
        .connection_presence()
        .identity()
        .filter(identity)
        .next()
        .is_some()
}

fn active_room_id(ctx: &ViewContext) -> Option<String> {
    let active = ctx.db.active_room().identity().find(&ctx.sender())?;
    ctx.db
        .member()
        .member_key()
        .find(&member_key(&active.room_id, ctx.sender()))
        .map(|_| active.room_id)
}

fn activate_room(ctx: &ReducerContext, room_id: String) {
    if let Some(mut previous) = ctx.db.active_room().identity().find(&ctx.sender()) {
        if previous.room_id != room_id {
            let old_key = member_key(&previous.room_id, ctx.sender());
            if let Some(mut member) = ctx.db.member().member_key().find(&old_key) {
                member.connected = false;
                ctx.db.member().member_key().update(member);
            }
        }
        previous.room_id = room_id;
        ctx.db.active_room().identity().update(previous);
    } else {
        ctx.db.active_room().insert(ActiveRoom {
            identity: ctx.sender(),
            room_id,
        });
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

fn caller_seat(ctx: &ReducerContext, room_id: &str) -> Result<u8, String> {
    ctx.db
        .member()
        .member_key()
        .find(&member_key(room_id, ctx.sender()))
        .ok_or_else(|| "join the room before acting".to_string())?
        .seat
        .ok_or_else(|| "take a seat before acting".to_string())
}

fn initial_game(seed: u64) -> Result<Game<PLAYERS>, String> {
    let dealer = Seat::new(0).map_err(rule_error)?;
    let game = OracleEnvironment::<PLAYERS>::initial(dealer).map_err(rule_error)?;
    transition_game(
        &game,
        EnvironmentAction::Chance(OracleChanceAction::seeded(seed, 0)),
    )
}

fn reconstruct_game(ctx: &ReducerContext, record: &RoomGame) -> Result<Game<PLAYERS>, String> {
    let mut game = initial_game(record.seed)?;
    let mut actions = ctx
        .db
        .game_action()
        .room_id()
        .filter(&record.room_id)
        .collect::<Vec<_>>();
    actions.sort_by_key(|action| action.sequence);
    if u64::try_from(actions.len()).ok() != Some(record.action_count) {
        return Err("game action log does not match its projection revision".into());
    }
    for (expected, action) in actions.into_iter().enumerate() {
        if action.sequence != u64::try_from(expected).unwrap_or(u64::MAX) {
            return Err("game action log is not contiguous".into());
        }
        game =
            round_flow::replay_action(&game, record.seed, action.seat, &action.kind, action.value)?;
    }
    Ok(game)
}

fn transition_game(
    game: &Game<PLAYERS>,
    action: EnvironmentAction<OraclePlayerAction<PLAYERS>, OracleChanceAction>,
) -> Result<Game<PLAYERS>, String> {
    OracleEnvironment::<PLAYERS>::transition(game, action)
        .map(|outcome| outcome.state)
        .map_err(rule_error)
}

/// Idempotent score recording is separate from the payment-gated phase change.
fn ensure_score_record(ctx: &ReducerContext, room: &str) -> Result<(), String> {
    let Some(record) = ctx.db.room_game().room_id().find(&room.to_string()) else {
        return Ok(());
    };
    if record.phase != "scoring" {
        return Ok(());
    }
    let key = format!("{room}:{}", record.round_index);
    if ctx.db.round_record().round_key().find(&key).is_some() {
        return Ok(());
    }
    let game = reconstruct_game(ctx, &record)?;
    let view = game.observe(Seat::new(0).map_err(rule_error)?);
    let scores = std::array::from_fn::<_, 2, _>(|seat| {
        poche_oracle_rust::score_round(
            view.bids[seat].expect("scoring has all bids"),
            view.tricks_won[seat],
            view.hand_size,
        )
    });
    let totals = round_flow::scored_totals(&game);
    ctx.db.round_record().insert(RoundRecord {
        round_key: key,
        room_id: room.into(),
        round_index: record.round_index,
        dealer_seat: record.dealer_seat.expect("scoring has dealer"),
        hand_size: view.hand_size,
        bids: view.bids.map(|bid| bid.expect("scoring has bids")).to_vec(),
        tricks_won: view.tricks_won.to_vec(),
        points: scores.map(|score| score.points).to_vec(),
        totals: totals.to_vec(),
        payment_cents: scores.map(|score| score.payment_cents).to_vec(),
    });
    write_room_game(ctx, record, &game)?;
    append_activity(
        ctx,
        room,
        "round-scored",
        format!(
            "Round {} scored: {} and {} points; running totals {} and {}",
            view.round_index + 1,
            scores[0].points,
            scores[1].points,
            totals[0],
            totals[1]
        ),
    );
    Ok(())
}

/// Settles once after all real coins are paid, but never deals on a player's behalf.
fn settle_if_paid(ctx: &ReducerContext, room: &str) -> Result<(), String> {
    ensure_score_record(ctx, room)?;
    let Some(mut record) = ctx.db.room_game().room_id().find(&room.to_string()) else {
        return Ok(());
    };
    if record.phase != "scoring" {
        return Ok(());
    }
    let members = ctx
        .db
        .member()
        .room_id()
        .filter(room)
        .filter(|member| member.seat.is_some())
        .collect::<Vec<_>>();
    if members.len() != PLAYERS
        || members
            .iter()
            .any(|member| money::payment_due(ctx, room, member.identity, member.seat.unwrap()) != 0)
    {
        return Ok(());
    }
    let game = reconstruct_game(ctx, &record)?;
    let next = transition_game(&game, EnvironmentAction::Settle)?;
    append_action(ctx, &record, 0, "settle", 0);
    record.action_count += 1;
    write_room_game(ctx, record, &next)?;
    clear_round_cards(ctx, room);
    let view = next.observe(Seat::new(0).map_err(rule_error)?);
    if view.phase == PhaseTag::Finished {
        append_activity(
            ctx,
            room,
            "game-finished",
            format!(
                "All rounds are complete; final scores {} and {}. The bowl is reserved for the winner(s); payout is not automated",
                view.scores[0], view.scores[1]
            ),
        );
    } else {
        let dealer = view.dealer.expect("awaiting deal has dealer").index();
        let name = members
            .iter()
            .find(|member| member.seat == u8::try_from(dealer).ok())
            .map_or("the next dealer", |member| member.display_name.as_str());
        append_activity(
            ctx,
            room,
            "round-settled",
            format!(
                "All payments settled; cards returned to the deck. {name} can deal round {} by clicking the deck",
                view.round_index + 1
            ),
        );
    }
    Ok(())
}

fn append_action(ctx: &ReducerContext, record: &RoomGame, seat: u8, kind: &str, value: u8) {
    ctx.db.game_action().insert(GameAction {
        action_key: format!("{}:{}", record.room_id, record.action_count),
        room_id: record.room_id.clone(),
        sequence: record.action_count,
        seat,
        kind: kind.to_string(),
        value,
    });
}

fn write_room_game(
    ctx: &ReducerContext,
    record: RoomGame,
    game: &Game<PLAYERS>,
) -> Result<(), String> {
    let mut projection =
        project_room_game(&record.room_id, record.seed, record.action_count, game)?;
    projection.pot_cents = money::pot_cents(ctx, &record.room_id);
    ctx.db.room_game().room_id().update(projection);
    Ok(())
}

fn project_room_game(
    room_id: &str,
    seed: u64,
    action_count: u64,
    game: &Game<PLAYERS>,
) -> Result<RoomGame, String> {
    let observer = Seat::new(0).map_err(rule_error)?;
    let view = game.observe(observer);
    let trick = view.current_trick.iter().collect::<Vec<_>>();
    let actor_seat = match view.actor {
        Turn::Player(player) => {
            Some(u8::try_from(player.index()).map_err(|_| "actor seat exceeds u8".to_string())?)
        }
        Turn::Chance | Turn::Environment | Turn::Finished => None,
    };
    Ok(RoomGame {
        room_id: room_id.to_string(),
        seed,
        action_count,
        phase: phase_label(view.phase).to_string(),
        actor_seat,
        dealer_seat: view
            .dealer
            .map(|seat| u8::try_from(seat.index()))
            .transpose()
            .map_err(|_| "dealer seat exceeds u8".to_string())?,
        round_index: u16::try_from(view.round_index)
            .map_err(|_| "round index exceeds u16".to_string())?,
        hand_size: view.hand_size,
        hand_count_0: view.hand_counts[0],
        hand_count_1: view.hand_counts[1],
        bid_0: view.bids[0],
        bid_1: view.bids[1],
        trick_count: u8::try_from(trick.len()).map_err(|_| "trick count exceeds u8".to_string())?,
        trick_seat_0: trick
            .first()
            .and_then(|play| u8::try_from(play.player.index()).ok()),
        trick_card_0: trick.first().map(|play| card_code(play.card)).transpose()?,
        trick_seat_1: trick
            .get(1)
            .and_then(|play| u8::try_from(play.player.index()).ok()),
        trick_card_1: trick.get(1).map(|play| card_code(play.card)).transpose()?,
        tricks_won_0: view.tricks_won[0],
        tricks_won_1: view.tricks_won[1],
        score_0: round_flow::scored_totals(game)[0],
        score_1: round_flow::scored_totals(game)[1],
        pot_cents: view.pot_cents,
        trump: view.trump.map(card_code).transpose()?,
    })
}

fn finish_trick_poses(ctx: &ReducerContext, room_id: &str, winner: u8) {
    let mut cards = ctx
        .db
        .card_pose()
        .room_id()
        .filter(room_id)
        .filter(|pose| pose.logical_location.starts_with("play:"))
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| left.card_key.cmp(&right.card_key));
    let count = i32::try_from(cards.len()).unwrap_or_default();
    let previous_count = i32::try_from(
        ctx.db
            .card_pose()
            .room_id()
            .filter(room_id)
            .filter(|pose| pose.logical_location == format!("won:{winner}"))
            .count(),
    )
    .unwrap_or(0);
    for (index, mut pose) in cards.into_iter().enumerate() {
        pose.logical_location = format!("won:{winner}");
        pose.x_mm =
            (2 * i32::try_from(index).unwrap_or_default() + 1 - count) * 18 + previous_count * 4;
        pose.y_mm = 40 + previous_count / 2 * 3 + i32::try_from(index).unwrap_or_default();
        pose.z_mm = if winner == 0 {
            LAYOUT_WON_Z_MM
        } else {
            -LAYOUT_WON_Z_MM
        };
        pose.ry_mdeg = 0;
        pose.sequence = pose.sequence.saturating_add(1);
        pose.committed_at = ctx.timestamp;
        ctx.db.card_pose().card_key().update(pose);
    }
}

fn hand_position(seat: u8, slot: u8, count: u8) -> [i32; 3] {
    let x = (2 * i32::from(slot) + 1 - i32::from(count)) * 7;
    [
        x,
        40,
        if seat == 0 {
            LAYOUT_HAND_Z_MM
        } else {
            -LAYOUT_HAND_Z_MM
        },
    ]
}

fn seed_for_room(room_id: &str) -> u64 {
    room_id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn card_from_code(code: u8) -> Result<Card, String> {
    Card::standard_deck()
        .get(usize::from(code))
        .copied()
        .ok_or_else(|| "card code is outside the standard deck".to_string())
}

fn card_code(card: Card) -> Result<u8, String> {
    Card::standard_deck()
        .iter()
        .position(|candidate| *candidate == card)
        .and_then(|index| u8::try_from(index).ok())
        .ok_or_else(|| "card is outside the standard deck".to_string())
}

fn card_label(code: u8) -> Result<&'static str, String> {
    const LABELS: [&str; 52] = [
        "2♣", "3♣", "4♣", "5♣", "6♣", "7♣", "8♣", "9♣", "10♣", "J♣", "Q♣", "K♣", "A♣", "2♦", "3♦",
        "4♦", "5♦", "6♦", "7♦", "8♦", "9♦", "10♦", "J♦", "Q♦", "K♦", "A♦", "2♥", "3♥", "4♥", "5♥",
        "6♥", "7♥", "8♥", "9♥", "10♥", "J♥", "Q♥", "K♥", "A♥", "2♠", "3♠", "4♠", "5♠", "6♠", "7♠",
        "8♠", "9♠", "10♠", "J♠", "Q♠", "K♠", "A♠",
    ];
    LABELS
        .get(usize::from(code))
        .copied()
        .ok_or_else(|| "card code is outside the standard deck".to_string())
}

const fn phase_label(phase: PhaseTag) -> &'static str {
    match phase {
        PhaseTag::AwaitingDeal => "awaiting-deal",
        PhaseTag::Bidding => "bidding",
        PhaseTag::Playing => "playing",
        PhaseTag::Scoring => "scoring",
        PhaseTag::Finished => "finished",
    }
}

fn rule_error(error: poche_oracle_rust::RuleViolation) -> String {
    format!("Poche rule rejected the action: {error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_oracle_trick_is_replayed_through_the_pure_rules_engine() {
        let mut game = initial_game(7).expect("seeded first deal");
        for _ in 0..2 {
            let actor = match game.turn() {
                Turn::Player(actor) => actor,
                other => panic!("expected bidder, got {other:?}"),
            };
            game = transition_game(
                &game,
                EnvironmentAction::Player(OraclePlayerAction::Bid {
                    player: actor,
                    tricks: 0,
                }),
            )
            .expect("zero bid is legal in round one");
        }
        for _ in 0..2 {
            let actor = match game.turn() {
                Turn::Player(actor) => actor,
                other => panic!("expected card player, got {other:?}"),
            };
            let card = game
                .observe(actor)
                .private_hand
                .iter()
                .next()
                .expect("round-one hand has one card");
            game = transition_game(
                &game,
                EnvironmentAction::Player(OraclePlayerAction::Play {
                    player: actor,
                    card,
                }),
            )
            .expect("only round-one card is legal");
        }
        assert_eq!(game.observe(Seat::new(0).unwrap()).phase, PhaseTag::Scoring);
    }

    #[test]
    fn card_labels_cover_the_dense_standard_deck() {
        assert_eq!(card_label(0), Ok("2♣"));
        assert_eq!(card_label(51), Ok("A♠"));
        assert!(card_label(52).is_err());
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

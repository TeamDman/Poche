// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Private-table adapters for the renderer-neutral conserved coin contract.
use super::{
    Coin, Identity, ReducerContext, Table, active_room, append_activity, card_key, coin,
    ensure_deal, ensure_score_record, member, member_key, room_game, round_record, settle_if_paid,
};
use poche_money::{
    CoinContainer, coin_rest_pose, first_free_coin_pose, initial_inventory, validate_transfer,
};

pub(super) fn pot_cents(ctx: &ReducerContext, room: &str) -> u32 {
    ctx.db
        .coin()
        .room_id()
        .filter(room)
        .filter(|coin| coin.container == "bowl")
        .map(|coin| u32::from(coin.denomination_cents))
        .sum()
}

pub(super) fn paid_cents(ctx: &ReducerContext, room: &str, owner: Identity) -> u32 {
    ctx.db
        .coin()
        .owner()
        .filter(owner)
        .filter(|coin| coin.room_id == room && coin.container == "bowl")
        .map(|coin| u32::from(coin.denomination_cents))
        .sum()
}

fn needs_legacy_ante(has_game: bool, has_inventory: bool) -> bool {
    has_game && !has_inventory
}

/// Pre-coin releases charged the first quarter implicitly. Materialize that
/// existing charge from the newly allocated inventory once; never redeal cards
/// or give a newly created lobby a free automatic ante.
pub(super) fn migrate_legacy_room(ctx: &ReducerContext, room: &str) {
    if ctx
        .db
        .room_game()
        .room_id()
        .find(&room.to_string())
        .is_none()
    {
        return;
    }
    let seated = ctx
        .db
        .member()
        .room_id()
        .filter(room)
        .filter(|member| member.seat.is_some())
        .collect::<Vec<_>>();
    let mut migrated = false;
    for member in seated {
        let has_inventory = ctx
            .db
            .coin()
            .owner()
            .filter(member.identity)
            .any(|coin| coin.room_id == room);
        if !needs_legacy_ante(true, has_inventory) {
            continue;
        }
        let seat = member.seat.expect("filtered seated member");
        ensure_inventory(ctx, room, member.identity, seat);
        let mut quarter = ctx
            .db
            .coin()
            .coin_key()
            .find(&card_key(room, member.identity, "q-000"))
            .expect("new inventory includes its first quarter");
        let occupied = ctx
            .db
            .coin()
            .room_id()
            .filter(room)
            .filter(|coin| coin.container == "bowl")
            .map(|coin| [coin.x_mm, coin.y_mm, coin.z_mm])
            .collect::<Vec<_>>();
        let position = first_free_coin_pose(seat, CoinContainer::Bowl, 25, &occupied)
            .expect("the first deal bowl has space for two antes");
        quarter.container = "bowl".into();
        set_position(&mut quarter, position);
        quarter.sequence = quarter.sequence.saturating_add(1);
        ctx.db.coin().coin_key().update(quarter);
        migrated = true;
    }
    if migrated {
        if let Some(mut game) = ctx.db.room_game().room_id().find(&room.to_string()) {
            game.pot_cents = pot_cents(ctx, room);
            ctx.db.room_game().room_id().update(game);
        }
        append_activity(ctx,room,"legacy-ante-materialized",
            "The existing automatic antes are now represented by quarters from the players' coin inventories; the deal is unchanged".into());
    }
}

pub(super) fn payment_due(ctx: &ReducerContext, room: &str, owner: Identity, seat: u8) -> u32 {
    let obligations = 25
        + ctx
            .db
            .round_record()
            .room_id()
            .filter(room)
            .map(|row| row.payment_cents[usize::from(seat)])
            .sum::<u32>();
    obligations.saturating_sub(paid_cents(ctx, room, owner))
}

pub(super) fn ensure_inventory(ctx: &ReducerContext, room: &str, owner: Identity, seat: u8) {
    if ctx
        .db
        .coin()
        .owner()
        .filter(owner)
        .any(|coin| coin.room_id == room)
    {
        return;
    }
    let mut jar_index = 0;
    let mut lid_index = 0;
    for (coin_id, denomination, container) in initial_inventory() {
        let index = if container == CoinContainer::Jar {
            &mut jar_index
        } else {
            &mut lid_index
        };
        let [x_mm, y_mm, z_mm] = coin_rest_pose(seat, container, *index, denomination);
        *index += 1;
        ctx.db.coin().insert(Coin {
            coin_key: card_key(room, owner, &coin_id),
            room_id: room.into(),
            owner,
            coin_id,
            denomination_cents: denomination,
            container: container.as_str().into(),
            x_mm,
            y_mm,
            z_mm,
            sequence: 0,
            committed_at: ctx.timestamp,
        });
    }
}

pub(super) fn place_inventory(ctx: &ReducerContext, room: &str, owner: Identity, seat: u8) {
    let mut coins = ctx
        .db
        .coin()
        .owner()
        .filter(owner)
        .filter(|coin| coin.room_id == room && coin.container != "bowl")
        .collect::<Vec<_>>();
    coins.sort_by(|a, b| a.coin_id.cmp(&b.coin_id));
    let mut jar_index = 0;
    let mut lid_index = 0;
    for mut coin in coins {
        let container =
            CoinContainer::parse(&coin.container).expect("only valid stored containers");
        let index = if container == CoinContainer::Jar {
            &mut jar_index
        } else {
            &mut lid_index
        };
        let position = coin_rest_pose(seat, container, *index, coin.denomination_cents);
        *index += 1;
        set_position(&mut coin, position);
        coin.sequence = coin.sequence.saturating_add(1);
        coin.committed_at = ctx.timestamp;
        ctx.db.coin().coin_key().update(coin);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn move_coin(
    ctx: &ReducerContext,
    room: &str,
    coin_id: &str,
    sequence: u64,
    target: &str,
    position: [i32; 3],
    commit: bool,
) -> Result<(), String> {
    ensure_score_record(ctx, room)?;
    let caller = ctx
        .db
        .member()
        .member_key()
        .find(&member_key(room, ctx.sender()))
        .ok_or("join the room before moving coins")?;
    let seat = caller.seat.ok_or("take a seat before moving coins")?;
    if ctx
        .db
        .active_room()
        .identity()
        .find(&ctx.sender())
        .is_none_or(|active| active.room_id != room)
    {
        return Err("coin actions must target the active room".into());
    }
    let mut coin = ctx
        .db
        .coin()
        .coin_key()
        .find(&card_key(room, ctx.sender(), coin_id))
        .ok_or("coin is not owned by this identity")?;
    let destination = validate_coin_move(
        &coin,
        ctx.sender(),
        sequence,
        target,
        position,
        commit,
        payment_due(ctx, room, ctx.sender(), seat),
    )?;
    if commit {
        let occupied = ctx
            .db
            .coin()
            .room_id()
            .filter(room)
            .filter(|other| {
                other.coin_key != coin.coin_key
                    && other.container == destination.as_str()
                    && (destination == CoinContainer::Bowl || other.owner == coin.owner)
            })
            .map(|other| [other.x_mm, other.y_mm, other.z_mm])
            .collect::<Vec<_>>();
        let resting = first_free_coin_pose(seat, destination, coin.denomination_cents, &occupied)
            .ok_or("coin container has no free canonical slot")?;
        set_position(&mut coin, resting);
        coin.container = destination.as_str().into();
    } else {
        set_position(&mut coin, position);
    }
    coin.sequence = sequence;
    coin.committed_at = ctx.timestamp;
    let value = coin.denomination_cents;
    ctx.db.coin().coin_key().update(coin);
    if commit {
        append_activity(
            ctx,
            room,
            if destination == CoinContainer::Bowl {
                "coin-paid"
            } else {
                "coin-moved"
            },
            format!(
                "{} moved a {value}¢ coin to {}",
                caller.display_name,
                destination.as_str()
            ),
        );
        if let Some(mut game) = ctx.db.room_game().room_id().find(&room.to_string()) {
            game.pot_cents = pot_cents(ctx, room);
            ctx.db.room_game().room_id().update(game);
        }
        ensure_deal(ctx, room)?;
        settle_if_paid(ctx, room)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_coin_move(
    coin: &Coin,
    caller: Identity,
    sequence: u64,
    target: &str,
    position: [i32; 3],
    commit: bool,
    due: u32,
) -> Result<CoinContainer, String> {
    if coin.owner != caller {
        return Err("coin is not owned by this identity".into());
    }
    if sequence <= coin.sequence {
        return Err("stale coin sequence".into());
    }
    if position
        .into_iter()
        .any(|axis| axis.unsigned_abs() > 10_000)
    {
        return Err("coin pose is outside the tabletop envelope".into());
    }
    let source = CoinContainer::parse(&coin.container).ok_or("unknown stored coin container")?;
    let destination = CoinContainer::parse(target).ok_or("unknown coin destination")?;
    validate_transfer(coin.denomination_cents, source, destination, due, commit)?;
    Ok(destination)
}

fn set_position(coin: &mut Coin, position: [i32; 3]) {
    [coin.x_mm, coin.y_mm, coin.z_mm] = position;
}

pub(super) fn refund_owner(ctx: &ReducerContext, room: &str, owner: Identity, seat: u8) {
    let coins = ctx
        .db
        .coin()
        .owner()
        .filter(owner)
        .filter(|coin| coin.room_id == room && coin.container == "bowl")
        .collect::<Vec<_>>();
    for mut coin in coins {
        coin.container = "lid".into();
        let occupied = ctx
            .db
            .coin()
            .owner()
            .filter(owner)
            .filter(|other| other.room_id == room && other.container == "lid")
            .map(|other| [other.x_mm, other.y_mm, other.z_mm])
            .collect::<Vec<_>>();
        let position =
            first_free_coin_pose(seat, CoinContainer::Lid, coin.denomination_cents, &occupied)
                .expect("a conserved inventory has room for its own returned coins");
        set_position(&mut coin, position);
        coin.sequence = coin.sequence.saturating_add(1);
        coin.committed_at = ctx.timestamp;
        ctx.db.coin().coin_key().update(coin);
    }
}

pub(super) fn refund_all(ctx: &ReducerContext, room: &str) {
    let owners = ctx
        .db
        .coin()
        .room_id()
        .filter(room)
        .filter(|coin| coin.container == "bowl")
        .map(|coin| coin.owner)
        .collect::<std::collections::HashSet<_>>();
    for owner in owners {
        let seat = ctx
            .db
            .member()
            .member_key()
            .find(&member_key(room, owner))
            .and_then(|m| m.seat)
            .unwrap_or(0);
        refund_owner(ctx, room, owner, seat);
    }
}

pub(super) fn delete_room_inventory(ctx: &ReducerContext, room: &str) {
    for coin in ctx.db.coin().room_id().filter(room).collect::<Vec<_>>() {
        ctx.db.coin().coin_key().delete(&coin.coin_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb::Timestamp;
    fn coin() -> Coin {
        Coin {
            coin_key: "r:alice:q-000".into(),
            room_id: "r".into(),
            owner: Identity::ZERO,
            coin_id: "q-000".into(),
            denomination_cents: 25,
            container: "lid".into(),
            x_mm: 0,
            y_mm: 25,
            z_mm: 0,
            sequence: 7,
            committed_at: Timestamp::UNIX_EPOCH,
        }
    }
    #[test]
    fn actual_schema_rejects_impostor_stale_sequence_unknown_container_and_overpayment() {
        let coin = coin();
        assert!(validate_coin_move(&coin, Identity::ZERO, 8, "bowl", [0, 25, 0], true, 25).is_ok());
        assert!(
            validate_coin_move(
                &coin,
                Identity::from_byte_array([1; 32]),
                8,
                "bowl",
                [0, 25, 0],
                true,
                25
            )
            .is_err()
        );
        assert!(
            validate_coin_move(&coin, Identity::ZERO, 7, "bowl", [0, 25, 0], true, 25).is_err()
        );
        assert!(
            validate_coin_move(&coin, Identity::ZERO, 8, "wallet", [0, 25, 0], true, 25).is_err()
        );
        assert!(validate_coin_move(&coin, Identity::ZERO, 8, "bowl", [0, 25, 0], true, 0).is_err());
    }
    #[test]
    fn preview_cannot_pay_and_hostile_integer_min_pose_does_not_panic() {
        let coin = coin();
        assert!(validate_coin_move(&coin, Identity::ZERO, 8, "lid", [1, 49, 0], false, 25).is_ok());
        assert!(
            validate_coin_move(&coin, Identity::ZERO, 8, "bowl", [1, 49, 0], false, 25).is_err()
        );
        assert!(
            validate_coin_move(
                &coin,
                Identity::ZERO,
                8,
                "lid",
                [i32::MIN, 49, 0],
                false,
                25
            )
            .is_err()
        );
    }
    #[test]
    fn legacy_ante_is_only_for_existing_game_without_previously_granted_inventory() {
        assert!(needs_legacy_ante(true, false));
        assert!(!needs_legacy_ante(true, true));
        assert!(!needs_legacy_ante(false, false));
        assert!(!needs_legacy_ante(false, true));
    }
}

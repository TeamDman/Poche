// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounded play-money contract. These coins have no redemption or real-money API.
//! The first desktop deal uses one quarter ante and one dime per missed bid.

pub const QUARTER_CENTS: u8 = 25;
pub const DIME_CENTS: u8 = 10;
pub const QUARTERS_PER_PLAYER: u16 = 100;
pub const DIMES_PER_PLAYER: u16 = 100;
pub const LID_QUARTERS: u16 = 6;
pub const LID_DIMES: u16 = 5;
pub const INITIAL_BANKROLL_CENTS: u32 = 3_500;
pub const INITIAL_LID_CENTS: u32 = 200;
pub const TABLE_SURFACE_Y_MM: i32 = 20;
pub const JAR_HEIGHT_MM: i32 = 180;
pub const JAR_RADIUS_MM: i32 = 40;
pub const LID_RADIUS_MM: i32 = 43;
pub const BOWL_RADIUS_MM: i32 = 75;
pub const BOWL_DEPTH_MM: i32 = 40;
pub const COIN_THICKNESS_MM: i32 = 2;
pub const QUARTER_DIAMETER_MM: i32 = 24;
pub const DIME_DIAMETER_MM: i32 = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoinContainer {
    Jar,
    Lid,
    Bowl,
}

impl CoinContainer {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jar => "jar",
            Self::Lid => "lid",
            Self::Bowl => "bowl",
        }
    }
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "jar" => Some(Self::Jar),
            "lid" => Some(Self::Lid),
            "bowl" => Some(Self::Bowl),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentStage {
    Ante,
    Playing,
    Scoring { missed_bid: bool },
}

#[must_use]
pub const fn amount_due(stage: PaymentStage, already_paid_cents: u32) -> u32 {
    let expected = match stage {
        PaymentStage::Ante => QUARTER_CENTS as u32,
        PaymentStage::Scoring { missed_bid: true } => (QUARTER_CENTS + DIME_CENTS) as u32,
        PaymentStage::Playing | PaymentStage::Scoring { missed_bid: false } => 0,
    };
    expected.saturating_sub(already_paid_cents)
}

/// Reject partial/overpayments: a quarter pays an ante; a dime pays a miss.
/// Already-paid bowl coins cannot be withdrawn in this bounded first-deal slice.
///
/// # Errors
/// Returns a stable explanation for an invalid transfer.
pub fn validate_transfer(
    denomination: u8,
    from: CoinContainer,
    to: CoinContainer,
    due: u32,
    commit: bool,
) -> Result<(), &'static str> {
    if denomination != QUARTER_CENTS && denomination != DIME_CENTS {
        return Err("unknown coin denomination");
    }
    if from == CoinContainer::Bowl {
        return Err("paid bowl coins are locked until settlement or an abandoned deal refund");
    }
    if !commit {
        return if from == to {
            Ok(())
        } else {
            Err("a preview cannot change a coin's logical container")
        };
    }
    if to == CoinContainer::Bowl && (due == 0 || u32::from(denomination) != due) {
        return Err("pay exactly one quarter for an owed ante or one dime for an owed missed bid");
    }
    Ok(())
}

/// Container base center on the table. Seat one mirrors its personal objects.
#[must_use]
pub const fn container_center_mm(seat: u8, container: CoinContainer) -> [i32; 3] {
    let sign = if seat == 1 { -1 } else { 1 };
    match container {
        CoinContainer::Jar => [-330 * sign, TABLE_SURFACE_Y_MM, 520 * sign],
        CoinContainer::Lid => [-220 * sign, TABLE_SURFACE_Y_MM, 430 * sign],
        CoinContainer::Bowl => [-240, TABLE_SURFACE_Y_MM, 0],
    }
}

/// Stable, compact piles; six jar columns fill a tall narrow jar without putting
/// all coins at one pose. Containers are logical, so moving their poses does not
/// mint value. Positions are integer millimeters in both server and renderer.
#[must_use]
pub fn coin_rest_pose(
    seat: u8,
    container: CoinContainer,
    index: usize,
    denomination: u8,
) -> [i32; 3] {
    let center = container_center_mm(seat, container);
    let sign = if seat == 1 && container != CoinContainer::Bowl {
        -1
    } else {
        1
    };
    let slots: &[(i32, i32)] = match container {
        CoinContainer::Jar => &[
            (-13, -22),
            (13, -22),
            (-26, 0),
            (0, 0),
            (26, 0),
            (-13, 22),
            (13, 22),
        ],
        CoinContainer::Lid => &[
            (-13, -20),
            (13, -20),
            (-26, 3),
            (0, 3),
            (26, 3),
            (-13, 26),
            (13, 26),
        ],
        CoinContainer::Bowl => &[(-30, -22), (0, -22), (30, -22), (-30, 8), (0, 8), (30, 8)],
    };
    let slot = slots[index % slots.len()];
    let layer = i32::try_from(index / slots.len())
        .unwrap_or(i32::MAX)
        .min(70);
    let bottom = TABLE_SURFACE_Y_MM + 5;
    let thickness = if denomination == DIME_CENTS {
        2
    } else {
        COIN_THICKNESS_MM
    };
    let rise = if container == CoinContainer::Jar {
        6
    } else {
        thickness + 1
    };
    [
        center[0] + slot.0 * sign,
        bottom + layer * rise,
        center[2] + slot.1 * sign,
    ]
}

#[must_use]
pub fn first_free_coin_pose(
    seat: u8,
    container: CoinContainer,
    denomination: u8,
    occupied: &[[i32; 3]],
) -> Option<[i32; 3]> {
    (0..200)
        .map(|index| coin_rest_pose(seat, container, index, denomination))
        .find(|position| !occupied.contains(position))
}

/// Put larger coins at the bottom; IDs only break ties within a denomination.
/// A lexical ID sort instead puts every `d-*` dime below every `q-*` quarter.
#[must_use]
pub fn coin_stack_order(denomination: u8, id: &str) -> (std::cmp::Reverse<u8>, &str) {
    (std::cmp::Reverse(denomination), id)
}

/// Repair the exact old lexical jar layout only. Any moved/raised coin makes
/// the entire jar ineligible, so reconnecting cannot rearrange someone's drag.
/// IDs, denominations and ownership are unchanged; this is pose-only repair.
#[must_use]
pub fn legacy_jar_relayout(
    seat: u8,
    coins: &[(String, u8, [i32; 3])],
) -> Option<Vec<(String, [i32; 3])>> {
    let mut ordered = coins.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    if ordered
        .iter()
        .enumerate()
        .any(|(index, coin)| coin.2 != coin_rest_pose(seat, CoinContainer::Jar, index, coin.1))
    {
        return None;
    }
    ordered.sort_by(|a, b| coin_stack_order(a.1, &a.0).cmp(&coin_stack_order(b.1, &b.0)));
    let changed = ordered
        .iter()
        .enumerate()
        .filter_map(|(index, coin)| {
            let position = coin_rest_pose(seat, CoinContainer::Jar, index, coin.1);
            (position != coin.2).then(|| (coin.0.clone(), position))
        })
        .collect::<Vec<_>>();
    (!changed.is_empty()).then_some(changed)
}

#[must_use]
pub fn initial_inventory() -> Vec<(String, u8, CoinContainer)> {
    let mut coins = Vec::with_capacity(usize::from(QUARTERS_PER_PLAYER + DIMES_PER_PLAYER));
    for (prefix, denomination, total, on_lid) in [
        ("q", QUARTER_CENTS, QUARTERS_PER_PLAYER, LID_QUARTERS),
        ("d", DIME_CENTS, DIMES_PER_PLAYER, LID_DIMES),
    ] {
        for index in 0..total {
            coins.push((
                format!("{prefix}-{index:03}"),
                denomination,
                if index < on_lid {
                    CoinContainer::Lid
                } else {
                    CoinContainer::Jar
                },
            ));
        }
    }
    coins
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quarters_are_below_dimes_after_canonical_restack_for_both_seats() {
        let mut coins = initial_inventory()
            .into_iter()
            .filter(|coin| coin.2 == CoinContainer::Jar)
            .collect::<Vec<_>>();
        // Reproduce the old path: initial placement was correct, but taking a
        // seat immediately restacked IDs lexically and inverted denominations.
        coins.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(coins[0].1, DIME_CENTS);
        coins.sort_by(|a, b| coin_stack_order(a.1, &a.0).cmp(&coin_stack_order(b.1, &b.0)));
        for seat in 0..2 {
            let quarters = coins
                .iter()
                .enumerate()
                .filter(|(_, coin)| coin.1 == QUARTER_CENTS)
                .map(|(index, coin)| coin_rest_pose(seat, CoinContainer::Jar, index, coin.1)[1])
                .collect::<Vec<_>>();
            let dimes = coins
                .iter()
                .enumerate()
                .filter(|(_, coin)| coin.1 == DIME_CENTS)
                .map(|(index, coin)| coin_rest_pose(seat, CoinContainer::Jar, index, coin.1)[1])
                .collect::<Vec<_>>();
            assert!(quarters.iter().max().unwrap() <= dimes.iter().min().unwrap());
        }
    }

    fn legacy_jar(seat: u8) -> Vec<(String, u8, [i32; 3])> {
        let mut coins = initial_inventory()
            .into_iter()
            .filter(|coin| coin.2 == CoinContainer::Jar)
            .collect::<Vec<_>>();
        coins.sort_by(|a, b| a.0.cmp(&b.0));
        coins
            .into_iter()
            .enumerate()
            .map(|(index, (id, denomination, _))| {
                (
                    id,
                    denomination,
                    coin_rest_pose(seat, CoinContainer::Jar, index, denomination),
                )
            })
            .collect()
    }

    #[test]
    fn existing_jar_pose_migration_conserves_coin_identity_and_value_and_is_idempotent() {
        for seat in 0..2 {
            let mut coins = legacy_jar(seat);
            let identities = coins
                .iter()
                .map(|coin| (coin.0.clone(), coin.1))
                .collect::<Vec<_>>();
            let updates = legacy_jar_relayout(seat, &coins).expect("legacy layout is repaired");
            for (id, position) in updates {
                coins.iter_mut().find(|coin| coin.0 == id).unwrap().2 = position;
            }
            assert_eq!(
                identities,
                coins
                    .iter()
                    .map(|coin| (coin.0.clone(), coin.1))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                coins.iter().map(|coin| u32::from(coin.1)).sum::<u32>(),
                INITIAL_BANKROLL_CENTS - INITIAL_LID_CENTS
            );
            assert!(legacy_jar_relayout(seat, &coins).is_none());
        }
    }

    #[test]
    fn legacy_jar_repair_does_not_interrupt_a_lift_or_a_preview_outside_the_jar() {
        let mut lifted = legacy_jar(0);
        lifted[0].2[1] += 24;
        assert!(legacy_jar_relayout(0, &lifted).is_none());
        let mut moved = legacy_jar(0);
        moved[0].2[0] += 120;
        assert!(legacy_jar_relayout(0, &moved).is_none());
    }
    #[test]
    fn inventory_conserves_exact_bankroll_and_lid_allowance() {
        let coins = initial_inventory();
        assert_eq!(coins.len(), 200);
        assert_eq!(
            coins.iter().map(|c| u32::from(c.1)).sum::<u32>(),
            INITIAL_BANKROLL_CENTS
        );
        assert_eq!(
            coins
                .iter()
                .filter(|c| c.2 == CoinContainer::Lid)
                .map(|c| u32::from(c.1))
                .sum::<u32>(),
            INITIAL_LID_CENTS
        );
        assert_eq!(
            coins
                .iter()
                .map(|c| &c.0)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            200
        );
    }
    #[test]
    fn ante_does_not_allow_partial_unpayable_dime_or_double_payment() {
        let due = amount_due(PaymentStage::Ante, 0);
        assert!(validate_transfer(25, CoinContainer::Lid, CoinContainer::Bowl, due, true).is_ok());
        assert!(validate_transfer(10, CoinContainer::Lid, CoinContainer::Bowl, due, true).is_err());
        assert!(
            validate_transfer(
                25,
                CoinContainer::Lid,
                CoinContainer::Bowl,
                amount_due(PaymentStage::Ante, 25),
                true
            )
            .is_err()
        );
    }
    #[test]
    fn miss_is_one_dime_and_matched_bid_owes_nothing() {
        assert_eq!(
            amount_due(PaymentStage::Scoring { missed_bid: true }, 25),
            10
        );
        assert_eq!(
            amount_due(PaymentStage::Scoring { missed_bid: false }, 25),
            0
        );
        assert_eq!(
            amount_due(PaymentStage::Scoring { missed_bid: true }, 35),
            0
        );
    }
    #[test]
    fn bowl_is_locked_and_preview_never_pays() {
        assert!(validate_transfer(25, CoinContainer::Bowl, CoinContainer::Lid, 0, true).is_err());
        assert!(validate_transfer(25, CoinContainer::Lid, CoinContainer::Bowl, 25, false).is_err());
        assert!(validate_transfer(25, CoinContainer::Jar, CoinContainer::Jar, 25, false).is_ok());
    }
    #[test]
    fn piles_are_distinct_inside_vessels_and_same_world_bowl_for_both_seats() {
        let poses = (0..189)
            .map(|i| coin_rest_pose(0, CoinContainer::Jar, i, 25))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(poses.len(), 189);
        let center = container_center_mm(0, CoinContainer::Jar);
        for [x, y, z] in poses {
            assert!(
                (x - center[0]).pow(2) + (z - center[2]).pow(2)
                    <= (JAR_RADIUS_MM - QUARTER_DIAMETER_MM / 2).pow(2)
            );
            assert!(y < TABLE_SURFACE_Y_MM + JAR_HEIGHT_MM);
        }
        assert_eq!(
            container_center_mm(0, CoinContainer::Bowl),
            container_center_mm(1, CoinContainer::Bowl)
        );
        assert_eq!(
            coin_rest_pose(0, CoinContainer::Bowl, 2, 25),
            coin_rest_pose(1, CoinContainer::Bowl, 2, 25)
        );
    }
    #[test]
    fn removing_middle_coin_reuses_hole_instead_of_colliding_with_top() {
        let mut occupied = (0..11)
            .map(|i| coin_rest_pose(0, CoinContainer::Lid, i, 25))
            .collect::<Vec<_>>();
        let removed = occupied.remove(3);
        let added = first_free_coin_pose(0, CoinContainer::Lid, 25, &occupied).unwrap();
        assert_eq!(added, removed);
        assert!(!occupied.contains(&added));
        occupied.push(added);
        assert_eq!(
            occupied
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            11
        );
    }
}

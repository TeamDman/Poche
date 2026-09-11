// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/

//! Public room membership projected into the same table-local frame as cards.
//! These fixed anchors are presentation, not movement or seating authority.
use std::collections::HashSet;

use poche_spatial::{
    AabbMm, HalfExtentsMm, Point3Mm, PoseMm, SeatId, SpatialLayout, YawMilliDegrees,
};

use crate::MemberPresentation;

/// One actual member, including unseated and temporarily disconnected members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticipantAnchor {
    pub principal: String,
    pub display_name: String,
    pub seat: Option<SeatId>,
    pub ready: bool,
    pub connected: bool,
    pub pose: PoseMm,
}

impl ParticipantAnchor {
    /// Bounds contain the desktop's capsule, without consulting its renderer.
    #[must_use]
    pub fn bounds(&self) -> AabbMm {
        AabbMm::from_center(self.pose.translation, HalfExtentsMm::new(80, 180, 80))
            .expect("bounded participant pose")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbySpatialError {
    Membership,
    SeatMap,
    Geometry,
}

/// Realize only public membership; there is deliberately no hand/action input.
/// Stable principal ordering makes recipients independent of vector ordering.
///
/// # Errors
/// Rejects duplicate identities/seats, unsupported membership and intersecting
/// participant geometry rather than silently assigning or merging ownership.
pub fn realize_lobby_members(
    layout: &SpatialLayout,
    members: &[MemberPresentation],
) -> Result<Vec<ParticipantAnchor>, LobbySpatialError> {
    if members.len() > poche_protocol::MAX_PROTOCOL_PLAYERS {
        return Err(LobbySpatialError::Membership);
    }
    let mut ordered: Vec<_> = members.iter().collect();
    ordered.sort_by(|a, b| a.principal.cmp(&b.principal));
    let mut principals = HashSet::new();
    let mut seats = HashSet::new();
    let mut anchors: Vec<ParticipantAnchor> = Vec::with_capacity(members.len());
    for (index, member) in ordered.into_iter().enumerate() {
        if poche_protocol::PrincipalId::new(member.principal.clone()).is_err()
            || !principals.insert(&member.principal)
        {
            return Err(LobbySpatialError::Membership);
        }
        let seat = member
            .seat
            .map(|seat| SeatId::new(seat, layout.id()).ok_or(LobbySpatialError::SeatMap))
            .transpose()?;
        let pose = if let Some(seat) = seat {
            if !seats.insert(seat) {
                return Err(LobbySpatialError::SeatMap);
            }
            layout
                .seats()
                .iter()
                .find(|placement| placement.seat == seat)
                .ok_or(LobbySpatialError::SeatMap)?
                .player_pose
        } else {
            // Two standing rows beyond the table/seats. Index includes seated
            // members, so a seat change doesn't move other standing members.
            let x = if index % 2 == 0 { -1120 } else { 1120 };
            let z = -600 + i32::try_from(index / 2).map_err(|_| LobbySpatialError::Geometry)? * 400;
            PoseMm::new(
                Point3Mm::new(x, 240, z),
                YawMilliDegrees::new(if x < 0 { 90_000 } else { 270_000 }),
            )
        };
        if !pose.is_within_table_bounds() {
            return Err(LobbySpatialError::Geometry);
        }
        let anchor = ParticipantAnchor {
            principal: member.principal.clone(),
            display_name: member.display_name.clone(),
            seat,
            ready: member.ready,
            connected: member.connected,
            pose,
        };
        if anchors
            .iter()
            .any(|other| other.bounds().intersects(anchor.bounds()))
        {
            return Err(LobbySpatialError::Geometry);
        }
        anchors.push(anchor);
    }
    Ok(anchors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_spatial::{LayoutId, TableId, registered_layout};

    fn member(index: u8, seat: Option<u8>) -> MemberPresentation {
        MemberPresentation {
            principal: format!("member-{index}"),
            display_name: format!("Player {index}"),
            role: "spectator",
            seat,
            ready: false,
            connected: true,
        }
    }

    #[test]
    fn all_supported_membership_subsets_have_unique_bounded_capsules() {
        for players in 2..=8 {
            let layout =
                registered_layout(TableId::new(1), LayoutId::new(players, 1).unwrap()).unwrap();
            for seated_mask in 0..(1_u16 << players) {
                let members: Vec<_> = (0..8)
                    .map(|index| {
                        member(
                            index,
                            (index < players && seated_mask & (1 << index) != 0).then_some(index),
                        )
                    })
                    .collect();
                let anchors = realize_lobby_members(&layout, &members).unwrap();
                assert_eq!(anchors.len(), members.len());
                let mut reversed = members.clone();
                reversed.reverse();
                assert_eq!(anchors, realize_lobby_members(&layout, &reversed).unwrap());
                for anchor in &anchors {
                    assert!(anchor.bounds().min.is_within_table_bounds());
                    assert!(anchor.bounds().max.is_within_table_bounds());
                    if anchor.seat.is_none() {
                        let table = layout.table();
                        assert!(
                            !anchor.bounds().intersects(
                                AabbMm::from_center(table.pose.translation, table.half_extents)
                                    .unwrap()
                            )
                        );
                        for seat in layout.seats() {
                            assert!(
                                !anchor.bounds().intersects(
                                    AabbMm::from_center(
                                        seat.seat_pose.translation,
                                        HalfExtentsMm::new(220, 220, 220)
                                    )
                                    .unwrap()
                                )
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn seat_release_disconnect_and_departure_preserve_identity_without_phantoms() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 1).unwrap()).unwrap();
        assert!(realize_lobby_members(&layout, &[]).unwrap().is_empty());
        let mut members = vec![member(0, None), member(1, None)];
        let initial = realize_lobby_members(&layout, &members).unwrap();
        members[0].seat = Some(0);
        members[0].ready = true;
        let seated = realize_lobby_members(&layout, &members).unwrap();
        assert_eq!(seated[1], initial[1]);
        assert_eq!(seated[0].principal, initial[0].principal);
        assert_eq!(seated[0].pose, layout.seats()[0].player_pose);
        members[0].connected = false;
        let disconnected = realize_lobby_members(&layout, &members).unwrap();
        assert_eq!(disconnected[0].pose, seated[0].pose);
        assert!(!disconnected[0].connected);
        members[0].seat = None;
        members[0].ready = false;
        members[0].connected = true;
        assert_eq!(realize_lobby_members(&layout, &members).unwrap(), initial);
        members.remove(0);
        assert_eq!(realize_lobby_members(&layout, &members).unwrap().len(), 1);
    }

    #[test]
    fn malformed_membership_does_not_silently_grant_a_seat() {
        let layout = registered_layout(TableId::new(1), LayoutId::new(2, 1).unwrap()).unwrap();
        for members in [
            vec![member(0, None), member(0, None)],
            (0..9).map(|index| member(index, None)).collect(),
        ] {
            assert_eq!(
                realize_lobby_members(&layout, &members),
                Err(LobbySpatialError::Membership)
            );
        }
        for members in [
            vec![member(0, Some(2))],
            vec![member(0, Some(0)), member(1, Some(0))],
        ] {
            assert_eq!(
                realize_lobby_members(&layout, &members),
                Err(LobbySpatialError::SeatMap)
            );
        }
    }
}

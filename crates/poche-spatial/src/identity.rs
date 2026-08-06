// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{MAX_LAYOUT_PLAYERS, MIN_LAYOUT_PLAYERS};

/// Stable identity of one table-local coordinate frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableId(u64);

impl TableId {
    /// Construct a table identity from an application-assigned value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the application-assigned value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable identifier for one registered spatial layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId {
    players: u8,
    revision: u16,
}

impl LayoutId {
    /// Construct a supported layout identifier.
    #[must_use]
    pub const fn new(players: u8, revision: u16) -> Option<Self> {
        if players >= MIN_LAYOUT_PLAYERS && players <= MAX_LAYOUT_PLAYERS && revision > 0 {
            Some(Self { players, revision })
        } else {
            None
        }
    }

    /// Return the number of seat/hand anchors in the layout.
    #[must_use]
    pub const fn players(self) -> u8 {
        self.players
    }

    /// Return the layout revision.
    #[must_use]
    pub const fn revision(self) -> u16 {
        self.revision
    }
}

/// Stable zero-based seat identifier validated against a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeatId(u8);

impl SeatId {
    /// Construct a seat belonging to `layout`.
    #[must_use]
    pub const fn new(value: u8, layout: LayoutId) -> Option<Self> {
        if value < layout.players() {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Return the zero-based seat value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Opaque card-object identity within one viewer projection epoch.
///
/// This identity deliberately does not encode a card face. A shuffle may issue
/// new unlinkable object identities so public spatial trajectories do not reveal
/// future hidden identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardObjectId {
    /// Projection/privacy epoch that allocated the object handle.
    pub projection_epoch: u64,
    /// Dense object ordinal within that projection epoch.
    pub ordinal: u8,
}

/// Semantic card and interaction zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZoneId {
    /// Draw/deal deck.
    Deck,
    /// Face-up trump card separated from the draw deck.
    Trump,
    /// One player's private hand region.
    Hand(SeatId),
    /// Central current-trick/play region.
    Play,
    /// Tricks already won by one player.
    Won(SeatId),
}

/// Stable semantic identity of a spatial object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectId {
    /// Physical table surface.
    Table,
    /// Seat anchor for one player.
    Seat(SeatId),
    /// Player/token anchor for one seat.
    Player(SeatId),
    /// Semantic snap/display volume.
    Zone(ZoneId),
    /// Physical score sheet.
    ScoreSheet,
    /// Opaque card object.
    Card(CardObjectId),
}

/// Addressable surface of a semantic object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    /// Upward/front face used for card and score text.
    Face,
    /// Card back or reverse surface.
    Back,
    /// Generic top surface.
    Top,
}

/// Exact semantic surface attachment target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId {
    /// Owning semantic object.
    pub object: ObjectId,
    /// Surface on that object.
    pub kind: SurfaceKind,
}

/// Stable text-run identity inside one spatial scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRunId(pub u16);

#[cfg(test)]
mod tests {
    use super::{LayoutId, SeatId};

    #[test]
    fn first_contract_supports_layouts_for_two_through_eight_players() {
        assert!(LayoutId::new(1, 1).is_none());
        for players in 2..=8 {
            let layout = LayoutId::new(players, 1).expect("supported player count");
            assert!(SeatId::new(players - 1, layout).is_some());
            assert!(SeatId::new(players, layout).is_none());
        }
        assert!(LayoutId::new(9, 1).is_none());
        assert!(LayoutId::new(2, 0).is_none());
    }
}

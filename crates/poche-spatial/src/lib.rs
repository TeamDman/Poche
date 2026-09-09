// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Engine-neutral spatial refinement types for Poche.
//!
//! Typed game/session state remains canonical. This crate describes a
//! deterministic, viewer-scoped realization of that state and the contracts
//! used to classify spatial input back into typed intent. It deliberately has
//! no renderer, window, network, protocol-codec, or reinforcement-learning
//! dependency.

mod abstraction;
mod hash;
mod identity;
mod interaction;
mod layout;
mod manipulation;
mod realization;
mod scene;
mod units;

pub use abstraction::{AbstractionError, abstract_viewer_scene};
pub use hash::{spatial_scene_hash, spatial_scene_hash_hex};
pub use identity::{
    CardObjectId, LayoutId, ObjectId, SeatId, SurfaceId, SurfaceKind, TableId, TextRunId, ZoneId,
};
pub use interaction::{
    InteractionFinding, ResolvedCardPlay, SpatialPlayRecord, reconstruct_animation_endpoint,
    resolve_card_play, resolve_drag_play,
};
pub use layout::{
    LayoutError, SeatPlacement, SpatialLayout, ZoneClassification, ZoneVolume, registered_layout,
};
pub use manipulation::{
    CardManipulation, ManipulationError, ManipulationPose, RotationMilliDegrees, hand_owner,
};
pub use realization::{
    PlayedCardProjection, PlayerSpatialProjection, RealizationError, RevealedWonCard,
    ScoreSheetError, ViewerSpatialProjection, interpret_score_sheet, realize_viewer_scene,
};
pub use scene::{
    AnimationEasing, AnimationEndpoint, CardFace, CardLocation, CardObject, SceneError,
    SceneObject, SceneObjectKind, SpatialScene, TabletopMode, TextBinding, TextRun,
};
pub use units::{AabbMm, HalfExtentsMm, Millimeters, Point3Mm, PoseMm, YawMilliDegrees};

/// Stable semantic identifier for the first spatial contract.
pub const SPATIAL_SCHEMA_ID: &str = "poche-spatial-v1";

/// Numeric version carried by every spatial scene.
pub const SPATIAL_SCHEMA_VERSION: u16 = 1;

/// Smallest player count supported by the spatial layout contract.
pub const MIN_LAYOUT_PLAYERS: u8 = 2;

/// Largest player count supported by the first spatial layout contract.
pub const MAX_LAYOUT_PLAYERS: u8 = 8;

/// Largest absolute table-local coordinate accepted by the v1 semantic scene.
pub const MAX_ABS_TABLE_COORDINATE_MM: i32 = 10_000;

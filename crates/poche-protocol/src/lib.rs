// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Facet's generated reflection methods make Clippy conservatively flag every
// colocated Serde Deserialize derive. Decoded wire values are never trusted
// directly: the strict codec runs the semantic validators before returning.
#![allow(clippy::unsafe_derive_deserialize)]

//! Facet-reflected, versioned session protocol with strict canonical NDJSON.
//!
//! The inspectable JSON representation and canonical signing representation
//! are deliberately separate. Every decoder is bounded and fail-closed before
//! a command reaches authorization or reduction.

mod codec;
mod device_cooperation;
mod gateway;
mod governance;
mod ids;
mod replication;
mod secret;
mod types;

pub use codec::*;
pub use device_cooperation::*;
pub use gateway::*;
pub use governance::*;
pub use ids::*;
pub use replication::*;
pub use secret::SecretKeyMaterial;
pub use types::*;

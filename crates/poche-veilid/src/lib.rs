// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Veilid-facing application identity and transport adapters.
//!
//! Application signing and recipient-encryption identities are stable across
//! replaceable Veilid node IDs and private routes. The `veilid` feature is
//! deliberately non-default; pure game, session, CLI, and RL paths never load
//! the network stack.

#[cfg(feature = "device-transport")]
mod device_transport;
mod identity;
mod membership;
mod projection_packet;
mod rendezvous;
mod room_code;
mod room_route_code;
mod store;
mod transport;
#[cfg(feature = "device-transport")]
pub use device_transport::*;
#[cfg(feature = "device-service")]
mod device_service;
#[cfg(feature = "device-service")]
pub use device_service::*;

pub use identity::*;
pub use membership::*;
pub use projection_packet::*;
pub use rendezvous::*;
pub use room_code::*;
pub use room_route_code::*;
pub use store::*;
pub use transport::*;

#[cfg(feature = "veilid")]
mod veilid_membership_store;
#[cfg(feature = "veilid")]
mod veilid_projection_crypto;
#[cfg(feature = "veilid")]
mod veilid_rendezvous;
#[cfg(feature = "veilid")]
mod veilid_store;
#[cfg(feature = "veilid")]
pub use veilid_membership_store::*;
#[cfg(feature = "veilid")]
pub use veilid_projection_crypto::*;
#[cfg(feature = "veilid")]
pub use veilid_rendezvous::*;
#[cfg(feature = "veilid")]
pub use veilid_store::*;

#[cfg(test)]
mod redemption_tests;

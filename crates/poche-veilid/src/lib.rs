// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Veilid-facing application identity and transport adapters.
//!
//! Application signing and recipient-encryption identities are stable across
//! replaceable Veilid node IDs and private routes. The `veilid` feature is
//! deliberately non-default; pure game, session, CLI, and RL paths never load
//! the network stack.

mod identity;
mod membership;
mod rendezvous;
mod room_code;
mod store;

pub use identity::*;
pub use membership::*;
pub use rendezvous::*;
pub use room_code::*;
pub use store::*;

#[cfg(feature = "veilid")]
mod veilid_membership_store;
#[cfg(feature = "veilid")]
mod veilid_rendezvous;
#[cfg(feature = "veilid")]
mod veilid_store;
#[cfg(feature = "veilid")]
pub use veilid_membership_store::*;
#[cfg(feature = "veilid")]
pub use veilid_rendezvous::*;
#[cfg(feature = "veilid")]
pub use veilid_store::*;

#[cfg(test)]
mod redemption_tests;

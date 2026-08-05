// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic, network-free reinforcement-learning boundary for Poche.
//!
//! This crate projects the policy-neutral [`poche_environment`] reducer into
//! immutable observation/action/reward specifications. It deliberately has no
//! protocol, room, renderer, Veilid, or learner dependency.

mod batch;
mod environment;
mod evaluation;
mod policy;
mod spec;
mod transcript;

pub use batch::*;
pub use environment::*;
pub use evaluation::*;
pub use policy::*;
pub use spec::*;
pub use transcript::*;

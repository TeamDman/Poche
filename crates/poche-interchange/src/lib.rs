// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned Phon interchange for cross-model fixtures, traces, and evidence.
//!
//! Wire shapes are intentionally distinct from the strongly refined in-memory
//! Poche model. Decode is followed by semantic validation before data is trusted.

mod session;
mod spatial;
mod validate;
mod wire;

pub use session::*;
pub use spatial::*;
pub use validate::{ValidatedEvidence, ValidationError};
pub use wire::*;

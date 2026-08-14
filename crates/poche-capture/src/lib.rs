// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Common capture validation, captioning, privacy, and persistence.
//!
//! Renderers return bytes and metadata. Only this crate chooses evidence
//! paths, normalizes content, computes hashes, and publishes manifests.

#![allow(
    clippy::missing_errors_doc,
    reason = "the public pipeline returns the documented stable redacted CapturePipelineError categories"
)]

mod key_wrap;
mod pipeline;
mod relay;
mod transfer;

pub use key_wrap::*;
pub use pipeline::*;
pub use relay::*;
pub use transfer::*;

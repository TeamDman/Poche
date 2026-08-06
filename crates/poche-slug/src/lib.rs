// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Renderer-neutral Slug font-outline and analytic-coverage primitives.
//!
//! Font bytes are supplied by callers. GPU allocation and renderer lifecycle
//! deliberately remain outside this crate.

mod slug;

pub use slug::*;

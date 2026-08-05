// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Burn-backed learning for the framework-neutral `poche-rl` contract.
//!
//! This crate owns tensors, PPO/GAE, checkpoints, and learned-policy
//! scheduling. It never owns or duplicates game rules.

mod checkpoint;
mod learner;
mod math;
mod model;
mod policy;
mod self_play;
mod training;

pub use checkpoint::*;
pub use learner::*;
pub use math::*;
pub use model::*;
pub use policy::*;
pub use self_play::*;
pub use training::*;

pub type CpuBackend = burn::backend::Autodiff<burn::backend::Flex>;
pub type GpuBackend = burn::backend::Autodiff<burn::backend::Wgpu>;

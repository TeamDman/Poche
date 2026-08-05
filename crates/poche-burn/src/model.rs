// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use burn::{
    nn::{Linear, LinearConfig, Relu},
    prelude::*,
};
use poche_rl::{ACTION_COUNT, OBSERVATION_SIZE};

/// Backend-generic shared-trunk actor-critic.
#[derive(Module, Debug)]
pub struct ActorCritic<B: Backend> {
    input: Linear<B>,
    hidden: Linear<B>,
    policy: Linear<B>,
    value: Linear<B>,
    activation: Relu,
}

/// Stable model shape recorded in every run/checkpoint manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorCriticConfig {
    pub observation_size: usize,
    pub hidden_size: usize,
    pub action_count: usize,
}

impl ActorCriticConfig {
    #[must_use]
    pub const fn poche_v1(hidden_size: usize) -> Self {
        Self {
            observation_size: OBSERVATION_SIZE,
            hidden_size,
            action_count: ACTION_COUNT,
        }
    }

    #[must_use]
    pub fn init<B: Backend>(&self, device: &B::Device) -> ActorCritic<B> {
        ActorCritic {
            input: LinearConfig::new(self.observation_size, self.hidden_size).init(device),
            hidden: LinearConfig::new(self.hidden_size, self.hidden_size).init(device),
            policy: LinearConfig::new(self.hidden_size, self.action_count).init(device),
            value: LinearConfig::new(self.hidden_size, 1).init(device),
            activation: Relu::new(),
        }
    }
}

/// Batched policy logits and scalar values.
pub struct ActorCriticOutput<B: Backend> {
    pub logits: Tensor<B, 2>,
    pub values: Tensor<B, 1>,
}

impl<B: Backend> ActorCritic<B> {
    /// Forward one host/device batch without policy masking.
    #[must_use]
    pub fn forward(&self, observations: Tensor<B, 2>) -> ActorCriticOutput<B> {
        let features = self.activation.forward(self.input.forward(observations));
        let features = self.activation.forward(self.hidden.forward(features));
        ActorCriticOutput {
            logits: self.policy.forward(features.clone()),
            values: self.value.forward(features).squeeze_dim(1),
        }
    }
}

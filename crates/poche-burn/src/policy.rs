// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use burn::{prelude::*, tensor::TensorData};
use poche_rl::{ACTION_COUNT, ActionIndex, DecisionView, OBSERVATION_SIZE, Policy, PolicyError};

use crate::ActorCritic;

/// Deterministic greedy inference adapter. Training sampling is recorded by
/// PPO rollout code; evaluation deliberately has no sampling variance.
pub struct GreedyBurnPolicy<B: Backend> {
    model: ActorCritic<B>,
    device: B::Device,
}

impl<B: Backend> GreedyBurnPolicy<B> {
    #[must_use]
    pub const fn new(model: ActorCritic<B>, device: B::Device) -> Self {
        Self { model, device }
    }
}

impl<B: Backend> Policy for GreedyBurnPolicy<B> {
    fn name(&self) -> &'static str {
        "burn-greedy-actor-critic-v1"
    }

    fn select(&mut self, decision: &DecisionView) -> Result<ActionIndex, PolicyError> {
        let input = Tensor::<B, 2>::from_data(
            TensorData::new(decision.observation.to_vec(), [1, OBSERVATION_SIZE]),
            &self.device,
        );
        let logits = self
            .model
            .forward(input)
            .logits
            .into_data()
            .to_vec::<f32>()
            .map_err(|_| PolicyError::NoLegalAction)?;
        let index = (0..ACTION_COUNT)
            .filter(|index| decision.legal_mask[*index])
            .max_by(|left, right| logits[*left].total_cmp(&logits[*right]))
            .ok_or(PolicyError::NoLegalAction)?;
        ActionIndex::new(index).map_err(|_| PolicyError::NoLegalAction)
    }
}

/// Stable host-side masked softmax used for deterministic sampling/evidence.
/// Illegal actions receive exactly zero probability.
#[must_use]
pub fn masked_probabilities(logits: &[f32], legal_mask: &[bool]) -> Vec<f32> {
    if logits.len() != legal_mask.len() || !legal_mask.iter().any(|legal| *legal) {
        return Vec::new();
    }
    let maximum = logits
        .iter()
        .zip(legal_mask)
        .filter(|(_, legal)| **legal)
        .map(|(logit, _)| *logit)
        .fold(f32::NEG_INFINITY, f32::max);
    let mut probabilities = logits
        .iter()
        .zip(legal_mask)
        .map(|(logit, legal)| {
            if *legal {
                (*logit - maximum).exp()
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    let total = probabilities.iter().sum::<f32>();
    for probability in &mut probabilities {
        *probability /= total;
    }
    probabilities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_assigns_exact_zero_to_every_illegal_action() {
        let probabilities = masked_probabilities(&[1000.0, 2.0, 3.0], &[false, true, true]);
        assert_eq!(probabilities[0].to_bits(), 0.0_f32.to_bits());
        assert!((probabilities.iter().sum::<f32>() - 1.0).abs() < 1.0e-6);
        assert!(probabilities[2] > probabilities[1]);
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// Preregistered PPO/GAE hyperparameters for the first learning run.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PpoConfig {
    pub gamma: f32,
    pub lambda: f32,
    pub clip: f32,
    pub value_coefficient: f32,
    pub entropy_coefficient: f32,
    pub gradient_norm_cap: f32,
    pub learning_rate: f64,
    pub epochs: usize,
    pub minibatch_size: usize,
}

impl Default for PpoConfig {
    fn default() -> Self {
        Self {
            gamma: 1.0,
            lambda: 0.95,
            clip: 0.2,
            value_coefficient: 0.5,
            entropy_coefficient: 0.01,
            gradient_norm_cap: 0.5,
            learning_rate: 3.0e-4,
            epochs: 4,
            minibatch_size: 256,
        }
    }
}

/// One GAE calculation row in same-seat decision time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaeInput {
    pub reward: f32,
    pub value: f32,
    pub next_value: f32,
    pub terminal: bool,
    pub decision_time_distance: u32,
}

/// Advantages and value targets corresponding exactly to input order.
#[derive(Clone, Debug, PartialEq)]
pub struct GaeOutput {
    pub advantages: Vec<f32>,
    pub returns: Vec<f32>,
}

/// Compute reverse-time GAE, resetting the trace at terminal rows and applying
/// discount once per intervening decision-time unit.
#[must_use]
pub fn generalized_advantage_estimate(rows: &[GaeInput], config: PpoConfig) -> GaeOutput {
    let mut advantages = vec![0.0; rows.len()];
    let mut trace = 0.0;
    for (index, row) in rows.iter().enumerate().rev() {
        let continuation = if row.terminal { 0.0 } else { 1.0 };
        let distance = i32::try_from(row.decision_time_distance).unwrap_or(i32::MAX);
        let gamma = config.gamma.powi(distance);
        let delta = row.reward + continuation * gamma * row.next_value - row.value;
        trace = delta + continuation * gamma * config.lambda * trace;
        advantages[index] = trace;
    }
    let returns = advantages
        .iter()
        .zip(rows)
        .map(|(advantage, row)| advantage + row.value)
        .collect();
    GaeOutput {
        advantages,
        returns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_calculated_gae_honors_terminal_and_decision_distance() {
        let config = PpoConfig {
            gamma: 0.5,
            lambda: 1.0,
            ..PpoConfig::default()
        };
        let result = generalized_advantage_estimate(
            &[
                GaeInput {
                    reward: 1.0,
                    value: 0.25,
                    next_value: 0.5,
                    terminal: false,
                    decision_time_distance: 2,
                },
                GaeInput {
                    reward: 2.0,
                    value: 0.5,
                    next_value: 99.0,
                    terminal: true,
                    decision_time_distance: 1,
                },
            ],
            config,
        );
        assert_eq!(result.advantages, vec![1.25, 1.5]);
        assert_eq!(result.returns, vec![1.5, 2.0]);
    }
}

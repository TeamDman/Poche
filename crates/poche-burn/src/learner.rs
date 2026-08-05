// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use burn::{
    module::{AutodiffModule, Module},
    optim::{
        Adam, AdamConfig, GradientsParams, Optimizer, adaptor::OptimizerAdaptor,
        grad_clipping::GradientClippingConfig,
    },
    prelude::*,
    record::{BinBytesRecorder, FullPrecisionSettings, Recorder},
    tensor::{ElementConversion, TensorData, activation::log_softmax, backend::AutodiffBackend},
};
use poche_rl::{ACTION_COUNT, OBSERVATION_SIZE};

use crate::{ActorCritic, ActorCriticConfig, PpoConfig};

pub type PpoOptimizer<B> = OptimizerAdaptor<Adam, ActorCritic<B>, B>;

/// One host-side PPO minibatch. All vectors are validated before device use.
#[derive(Clone, Debug, PartialEq)]
pub struct PpoBatch {
    pub observations: Vec<f32>,
    pub legal_masks: Vec<bool>,
    pub actions: Vec<usize>,
    pub old_log_probabilities: Vec<f32>,
    pub advantages: Vec<f32>,
    pub returns: Vec<f32>,
}

impl PpoBatch {
    /// Validate one flat, contiguous host batch.
    ///
    /// # Errors
    /// Returns a stable category for mismatched shapes, non-finite values, or
    /// masked selected actions.
    pub fn validate(&self) -> Result<usize, LearnerError> {
        let rows = self.actions.len();
        if rows == 0
            || self.observations.len() != rows * OBSERVATION_SIZE
            || self.legal_masks.len() != rows * ACTION_COUNT
            || self.old_log_probabilities.len() != rows
            || self.advantages.len() != rows
            || self.returns.len() != rows
        {
            return Err(LearnerError::Shape);
        }
        for (row, action) in self.actions.iter().copied().enumerate() {
            if action >= ACTION_COUNT || !self.legal_masks[row * ACTION_COUNT + action] {
                return Err(LearnerError::MaskedAction);
            }
        }
        if self
            .observations
            .iter()
            .chain(&self.old_log_probabilities)
            .chain(&self.advantages)
            .chain(&self.returns)
            .any(|value| !value.is_finite())
        {
            return Err(LearnerError::NonFinite);
        }
        Ok(rows)
    }

    /// Copy a deterministic minibatch by row index.
    ///
    /// # Errors
    /// Rejects an empty/out-of-range selection or an invalid source batch.
    pub fn select_rows(&self, indices: &[usize]) -> Result<Self, LearnerError> {
        let rows = self.validate()?;
        if indices.is_empty() || indices.iter().any(|index| *index >= rows) {
            return Err(LearnerError::Shape);
        }
        let mut selected = Self {
            observations: Vec::with_capacity(indices.len() * OBSERVATION_SIZE),
            legal_masks: Vec::with_capacity(indices.len() * ACTION_COUNT),
            actions: Vec::with_capacity(indices.len()),
            old_log_probabilities: Vec::with_capacity(indices.len()),
            advantages: Vec::with_capacity(indices.len()),
            returns: Vec::with_capacity(indices.len()),
        };
        for index in indices {
            selected.observations.extend_from_slice(
                &self.observations[index * OBSERVATION_SIZE..(index + 1) * OBSERVATION_SIZE],
            );
            selected.legal_masks.extend_from_slice(
                &self.legal_masks[index * ACTION_COUNT..(index + 1) * ACTION_COUNT],
            );
            selected.actions.push(self.actions[*index]);
            selected
                .old_log_probabilities
                .push(self.old_log_probabilities[*index]);
            selected.advantages.push(self.advantages[*index]);
            selected.returns.push(self.returns[*index]);
        }
        Ok(selected)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PpoMetrics {
    pub total_loss: f32,
    pub policy_loss: f32,
    pub value_loss: f32,
    pub entropy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnerError {
    Shape,
    MaskedAction,
    NonFinite,
    Record,
}

/// Backend-generic learner with Burn-owned optimizer state.
pub struct PpoLearner<B: AutodiffBackend> {
    pub model: ActorCritic<B>,
    pub optimizer: PpoOptimizer<B>,
    pub config: PpoConfig,
}

impl<B: AutodiffBackend> PpoLearner<B> {
    #[must_use]
    pub fn new(model_config: ActorCriticConfig, config: PpoConfig, device: &B::Device) -> Self {
        let optimizer = AdamConfig::new()
            .with_grad_clipping(Some(GradientClippingConfig::Norm(config.gradient_norm_cap)))
            .init();
        Self {
            model: model_config.init(device),
            optimizer,
            config,
        }
    }

    /// Apply one clipped PPO update with one batched host/device transfer.
    ///
    /// # Errors
    /// Rejects invalid batch shapes and masked selections before tensor work.
    pub fn update(
        &mut self,
        batch: &PpoBatch,
        device: &B::Device,
    ) -> Result<PpoMetrics, LearnerError> {
        let rows = batch.validate()?;
        let observations = Tensor::<B, 2>::from_data(
            TensorData::new(batch.observations.clone(), [rows, OBSERVATION_SIZE]),
            device,
        );
        let illegal = Tensor::<B, 2, Bool>::from_data(
            TensorData::new(batch.legal_masks.clone(), [rows, ACTION_COUNT]),
            device,
        )
        .bool_not();
        let actions = Tensor::<B, 1, Int>::from_data(
            TensorData::new(
                batch
                    .actions
                    .iter()
                    .map(|value| i64::try_from(*value).unwrap_or(i64::MAX))
                    .collect::<Vec<_>>(),
                [rows],
            ),
            device,
        )
        .unsqueeze_dim::<2>(1);
        let old_log_probabilities = Tensor::<B, 1>::from_data(
            TensorData::new(batch.old_log_probabilities.clone(), [rows]),
            device,
        );
        let advantages =
            Tensor::<B, 1>::from_data(TensorData::new(batch.advantages.clone(), [rows]), device);
        let returns =
            Tensor::<B, 1>::from_data(TensorData::new(batch.returns.clone(), [rows]), device);

        let output = self.model.forward(observations);
        let log_probabilities = log_softmax(output.logits.mask_fill(illegal, -1.0e9), 1);
        let selected = log_probabilities.clone().gather(1, actions).squeeze_dim(1);
        let ratio = (selected - old_log_probabilities).exp();
        let unclipped = ratio.clone() * advantages.clone();
        let clipped = ratio.clamp(1.0 - self.config.clip, 1.0 + self.config.clip) * advantages;
        let policy_loss = unclipped.min_pair(clipped).mean().neg();
        let value_loss = (output.values - returns).powf_scalar(2.0).mean();
        let probabilities = log_probabilities.clone().exp();
        let entropy = (probabilities * log_probabilities).sum_dim(1).mean().neg();
        let loss = policy_loss.clone()
            + value_loss.clone().mul_scalar(self.config.value_coefficient)
            - entropy.clone().mul_scalar(self.config.entropy_coefficient);
        let metrics = PpoMetrics {
            total_loss: loss.clone().into_scalar().elem::<f32>(),
            policy_loss: policy_loss.into_scalar().elem::<f32>(),
            value_loss: value_loss.into_scalar().elem::<f32>(),
            entropy: entropy.into_scalar().elem::<f32>(),
        };
        let gradients = GradientsParams::from_grads(loss.backward(), &self.model);
        self.model = self
            .optimizer
            .step(self.config.learning_rate, self.model.clone(), gradients);
        Ok(metrics)
    }

    #[must_use]
    pub fn valid(&self) -> ActorCritic<B::InnerBackend> {
        self.model.clone().valid()
    }

    /// Serialize model and optimizer records separately for artifact storage.
    ///
    /// # Errors
    /// Returns a stable record category without leaking bytes.
    pub fn record_bytes(&self) -> Result<(Vec<u8>, Vec<u8>), LearnerError> {
        let recorder = BinBytesRecorder::<FullPrecisionSettings>::default();
        let model = Recorder::<B>::record(&recorder, self.model.clone().into_record(), ())
            .map_err(|_| LearnerError::Record)?;
        let optimizer = Recorder::<B>::record(&recorder, self.optimizer.to_record(), ())
            .map_err(|_| LearnerError::Record)?;
        Ok((model, optimizer))
    }

    /// Restore model and optimizer state after semantic manifest validation.
    ///
    /// # Errors
    /// Returns a stable record category for malformed/incompatible bytes.
    pub fn load_record_bytes(
        mut self,
        model_bytes: Vec<u8>,
        optimizer_bytes: Vec<u8>,
        device: &B::Device,
    ) -> Result<Self, LearnerError> {
        let recorder = BinBytesRecorder::<FullPrecisionSettings>::default();
        let model_record: <ActorCritic<B> as Module<B>>::Record =
            Recorder::<B>::load(&recorder, model_bytes, device)
                .map_err(|_| LearnerError::Record)?;
        let optimizer_record: <PpoOptimizer<B> as Optimizer<ActorCritic<B>, B>>::Record = recorder
            .load(optimizer_bytes, device)
            .map_err(|_| LearnerError::Record)?;
        self.model = self.model.load_record(model_record);
        self.optimizer = self.optimizer.load_record(optimizer_record);
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use burn::backend::flex::FlexDevice;

    use super::*;
    use crate::CpuBackend;

    #[test]
    fn controlled_actor_critic_learns_preregistered_two_context_behavior() {
        let device = FlexDevice;
        CpuBackend::seed(&device, 41);
        let mut learner = PpoLearner::<CpuBackend>::new(
            ActorCriticConfig::poche_v1(16),
            PpoConfig {
                learning_rate: 0.02,
                entropy_coefficient: 0.0,
                ..PpoConfig::default()
            },
            &device,
        );
        let mut observations = vec![0.0; 2 * OBSERVATION_SIZE];
        observations[0] = 1.0;
        observations[OBSERVATION_SIZE + 1] = 1.0;
        let mut masks = vec![false; 2 * ACTION_COUNT];
        masks[0] = true;
        masks[1] = true;
        masks[ACTION_COUNT] = true;
        masks[ACTION_COUNT + 1] = true;
        for _ in 0..80 {
            learner
                .update(
                    &PpoBatch {
                        observations: observations.clone(),
                        legal_masks: masks.clone(),
                        actions: vec![0, 1],
                        old_log_probabilities: vec![-std::f32::consts::LN_2; 2],
                        advantages: vec![1.0; 2],
                        returns: vec![1.0; 2],
                    },
                    &device,
                )
                .unwrap();
        }
        let output = learner.valid().forward(Tensor::from_data(
            TensorData::new(observations, [2, OBSERVATION_SIZE]),
            &device,
        ));
        let logits = output.logits.into_data().to_vec::<f32>().unwrap();
        // PPO's 0.2 ratio clip intentionally stops pushing after the selected
        // probability crosses the clipped trust region. Greedy behavior, not
        // unbounded logit magnitude, is the preregistered optimum.
        assert!(logits[0] > logits[1]);
        assert!(logits[ACTION_COUNT + 1] > logits[ACTION_COUNT]);
    }

    #[test]
    fn cpu_model_and_optimizer_checkpoint_round_trip_exactly() {
        let device = FlexDevice;
        CpuBackend::seed(&device, 99);
        let learner = PpoLearner::<CpuBackend>::new(
            ActorCriticConfig::poche_v1(8),
            PpoConfig::default(),
            &device,
        );
        let input = Tensor::from_data(
            TensorData::new(vec![0.25; OBSERVATION_SIZE], [1, OBSERVATION_SIZE]),
            &device,
        );
        let before = learner
            .model
            .forward(input.clone())
            .logits
            .into_data()
            .to_vec::<f32>()
            .unwrap();
        let (model_bytes, optimizer_bytes) = learner.record_bytes().unwrap();
        let restored = PpoLearner::<CpuBackend>::new(
            ActorCriticConfig::poche_v1(8),
            PpoConfig::default(),
            &device,
        )
        .load_record_bytes(model_bytes, optimizer_bytes, &device)
        .unwrap();
        let after = restored
            .model
            .forward(input)
            .logits
            .into_data()
            .to_vec::<f32>()
            .unwrap();
        assert_eq!(before, after);
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::{Duration, Instant};

use crate::{ACTION_COUNT, ActionIndex, OBSERVATION_SIZE, PocheRlEnv, RlEnvironmentError};

/// Stable seed derivation for independent batch episodes.
#[must_use]
pub fn derive_episode_seed(root_seed: u64, environment: usize, episode: u64) -> u64 {
    let environment = u64::try_from(environment).unwrap_or(u64::MAX);
    let mut value = root_seed
        ^ environment.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ episode.wrapping_mul(0xd1b5_4a32_d192_ed03);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Result of one active environment slot in a batch step.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchStep {
    pub environment: usize,
    pub episode: u64,
    pub actor: usize,
    pub rewards: [f32; 2],
    pub terminal: bool,
    pub decision_count: u64,
}

/// Preallocated structure-of-arrays batch. It performs no serialization,
/// sockets, protocol construction, or dynamic allocation in `step` after the
/// result vector has reached batch capacity.
#[derive(Clone, Debug)]
pub struct PocheBatch {
    root_seed: u64,
    environments: Vec<PocheRlEnv>,
    episodes: Vec<u64>,
    seeds: Vec<u64>,
    observations: Vec<f32>,
    legal_masks: Vec<bool>,
    actions: Vec<usize>,
    rewards: Vec<[f32; 2]>,
    terminals: Vec<bool>,
    seats: Vec<u8>,
    results: Vec<BatchStep>,
    hot_buffer_reallocations: u64,
}

impl PocheBatch {
    /// Allocate all hot buffers exactly once.
    ///
    /// # Errors
    ///
    /// Returns the underlying typed environment reset error.
    pub fn new(size: usize, root_seed: u64) -> Result<Self, RlEnvironmentError> {
        let environments = (0..size)
            .map(|index| PocheRlEnv::reset(derive_episode_seed(root_seed, index, 0)))
            .collect::<Result<Vec<_>, _>>()?;
        let seeds = (0..size)
            .map(|index| derive_episode_seed(root_seed, index, 0))
            .collect();
        let mut value = Self {
            root_seed,
            environments,
            episodes: vec![0; size],
            seeds,
            observations: vec![0.0; size * OBSERVATION_SIZE],
            legal_masks: vec![false; size * ACTION_COUNT],
            actions: vec![0; size],
            rewards: vec![[0.0; 2]; size],
            terminals: vec![false; size],
            seats: vec![0; size],
            results: Vec::with_capacity(size),
            hot_buffer_reallocations: 0,
        };
        value.refresh();
        Ok(value)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.environments.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.environments.is_empty()
    }

    #[must_use]
    pub fn observations(&self) -> &[f32] {
        &self.observations
    }

    #[must_use]
    pub fn legal_masks(&self) -> &[bool] {
        &self.legal_masks
    }

    #[must_use]
    pub fn seats(&self) -> &[u8] {
        &self.seats
    }

    #[must_use]
    pub fn episodes(&self) -> &[u64] {
        &self.episodes
    }

    #[must_use]
    pub fn seeds(&self) -> &[u64] {
        &self.seeds
    }

    #[must_use]
    pub const fn hot_buffer_reallocations(&self) -> u64 {
        self.hot_buffer_reallocations
    }

    /// Select the first legal action in every slot without allocating.
    ///
    /// # Panics
    ///
    /// Panics only if an internal active environment violates the reducer
    /// contract by presenting no legal action at an agent turn.
    pub fn select_first_legal(&mut self) {
        for environment in 0..self.len() {
            let start = environment * ACTION_COUNT;
            self.actions[environment] = self.legal_masks[start..start + ACTION_COUNT]
                .iter()
                .position(|allowed| *allowed)
                .expect("active environment has a legal action");
        }
    }

    /// Copy exact fixed-vocabulary action indices into the next batch step.
    ///
    /// # Errors
    ///
    /// Returns an invalid index or length mismatch.
    pub fn set_actions(&mut self, actions: &[usize]) -> Result<(), RlEnvironmentError> {
        if actions.len() != self.len() {
            return Err(RlEnvironmentError::InvalidActionIndex);
        }
        for (target, action) in self.actions.iter_mut().zip(actions) {
            *target = ActionIndex::new(*action)?.get();
        }
        Ok(())
    }

    /// Apply one action per environment, reset terminal slots deterministically,
    /// and refresh the batched policy inputs.
    ///
    /// # Errors
    ///
    /// Returns an invalid/masked action or typed reducer failure.
    pub fn step(&mut self) -> Result<&[BatchStep], RlEnvironmentError> {
        let capacity = self.results.capacity();
        self.results.clear();
        for environment in 0..self.environments.len() {
            let actor = usize::from(self.seats[environment]);
            let action = ActionIndex::new(self.actions[environment])?;
            let result = self.environments[environment].step(action)?;
            self.rewards[environment] = result.instant_rewards;
            self.terminals[environment] = result.terminal;
            let decision_count = self.environments[environment].decision_count();
            self.results.push(BatchStep {
                environment,
                episode: self.episodes[environment],
                actor,
                rewards: result.instant_rewards,
                terminal: result.terminal,
                decision_count,
            });
            if result.terminal {
                self.episodes[environment] = self.episodes[environment].saturating_add(1);
                let seed =
                    derive_episode_seed(self.root_seed, environment, self.episodes[environment]);
                self.seeds[environment] = seed;
                self.environments[environment] = PocheRlEnv::reset(seed)?;
            }
        }
        if self.results.capacity() != capacity {
            self.hot_buffer_reallocations = self.hot_buffer_reallocations.saturating_add(1);
        }
        self.refresh();
        Ok(&self.results)
    }

    fn refresh(&mut self) {
        for (environment, env) in self.environments.iter().enumerate() {
            let decision = env.decision().expect("terminal slots reset immediately");
            let observation_start = environment * OBSERVATION_SIZE;
            self.observations[observation_start..observation_start + OBSERVATION_SIZE]
                .copy_from_slice(&decision.observation);
            let mask_start = environment * ACTION_COUNT;
            self.legal_masks[mask_start..mask_start + ACTION_COUNT]
                .copy_from_slice(&decision.legal_mask);
            self.seats[environment] =
                u8::try_from(decision.seat.index()).expect("two-player seat fits u8");
        }
    }
}

/// Structure-of-arrays turn transition buffer consumed by PPO/GAE. Each row
/// pairs one seat's action with that same seat's next observation.
#[derive(Clone, Debug)]
pub struct TurnTransitionBuffer {
    capacity: usize,
    len: usize,
    observations: Vec<f32>,
    next_observations: Vec<f32>,
    actions: Vec<usize>,
    rewards: Vec<f32>,
    terminals: Vec<bool>,
    decision_time_distances: Vec<u32>,
    seats: Vec<u8>,
    environments: Vec<usize>,
    episodes: Vec<u64>,
    seeds: Vec<u64>,
}

impl TurnTransitionBuffer {
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn observations(&self) -> &[f32] {
        &self.observations[..self.len * OBSERVATION_SIZE]
    }

    #[must_use]
    pub fn next_observations(&self) -> &[f32] {
        &self.next_observations[..self.len * OBSERVATION_SIZE]
    }

    #[must_use]
    pub fn actions(&self) -> &[usize] {
        &self.actions[..self.len]
    }

    #[must_use]
    pub fn rewards(&self) -> &[f32] {
        &self.rewards[..self.len]
    }

    #[must_use]
    pub fn terminals(&self) -> &[bool] {
        &self.terminals[..self.len]
    }

    #[must_use]
    pub fn decision_time_distances(&self) -> &[u32] {
        &self.decision_time_distances[..self.len]
    }

    #[must_use]
    pub fn seats(&self) -> &[u8] {
        &self.seats[..self.len]
    }

    #[must_use]
    pub fn environment_indices(&self) -> &[usize] {
        &self.environments[..self.len]
    }

    #[must_use]
    pub fn episodes(&self) -> &[u64] {
        &self.episodes[..self.len]
    }

    #[must_use]
    pub fn seeds(&self) -> &[u64] {
        &self.seeds[..self.len]
    }

    /// Reuse the allocated storage for the next rollout horizon.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

/// Batched rollout assembler with fixed pending slots for every environment and
/// seat. No observation vector is allocated during stepping.
#[derive(Clone, Debug)]
pub struct TurnBasedBatchRollout {
    batch: PocheBatch,
    pending_valid: Vec<bool>,
    pending_observations: Vec<f32>,
    pending_actions: Vec<usize>,
    pending_rewards: Vec<f32>,
    pending_distances: Vec<u32>,
    pending_episodes: Vec<u64>,
    pending_seeds: Vec<u64>,
    transitions: TurnTransitionBuffer,
}

impl TurnBasedBatchRollout {
    /// Allocate one batch plus a fixed completed-transition horizon.
    ///
    /// # Errors
    ///
    /// Returns an underlying deterministic environment reset failure.
    pub fn new(
        batch_size: usize,
        root_seed: u64,
        transition_capacity: usize,
    ) -> Result<Self, RlEnvironmentError> {
        let pending = batch_size * 2;
        Ok(Self {
            batch: PocheBatch::new(batch_size, root_seed)?,
            pending_valid: vec![false; pending],
            pending_observations: vec![0.0; pending * OBSERVATION_SIZE],
            pending_actions: vec![0; pending],
            pending_rewards: vec![0.0; pending],
            pending_distances: vec![0; pending],
            pending_episodes: vec![0; pending],
            pending_seeds: vec![0; pending],
            transitions: TurnTransitionBuffer {
                capacity: transition_capacity,
                len: 0,
                observations: vec![0.0; transition_capacity * OBSERVATION_SIZE],
                next_observations: vec![0.0; transition_capacity * OBSERVATION_SIZE],
                actions: vec![0; transition_capacity],
                rewards: vec![0.0; transition_capacity],
                terminals: vec![false; transition_capacity],
                decision_time_distances: vec![0; transition_capacity],
                seats: vec![0; transition_capacity],
                environments: vec![0; transition_capacity],
                episodes: vec![0; transition_capacity],
                seeds: vec![0; transition_capacity],
            },
        })
    }

    #[must_use]
    pub const fn batch(&self) -> &PocheBatch {
        &self.batch
    }

    #[must_use]
    pub const fn transitions(&self) -> &TurnTransitionBuffer {
        &self.transitions
    }

    pub fn transitions_mut(&mut self) -> &mut TurnTransitionBuffer {
        &mut self.transitions
    }

    /// Select first-legal actions for a deterministic correctness rollout.
    ///
    /// # Errors
    ///
    /// Returns an invalid action, reducer failure, or full transition buffer.
    pub fn step_first_legal(&mut self) -> Result<(), RlEnvironmentError> {
        self.batch.select_first_legal();
        self.step_selected()
    }

    /// Step exact actions already copied with [`PocheBatch::set_actions`].
    ///
    /// # Errors
    ///
    /// Returns an invalid action, reducer failure, or full transition buffer.
    pub fn step_selected(&mut self) -> Result<(), RlEnvironmentError> {
        let batch_size = self.batch.len();
        for environment in 0..batch_size {
            let seat = usize::from(self.batch.seats[environment]);
            let pending = environment * 2 + seat;
            if self.pending_valid[pending] {
                let next_start = environment * OBSERVATION_SIZE;
                let mut next = [0.0; OBSERVATION_SIZE];
                next.copy_from_slice(
                    &self.batch.observations[next_start..next_start + OBSERVATION_SIZE],
                );
                self.complete_pending(environment, seat, &next, false)?;
            }
            self.pending_valid[pending] = true;
            let source = environment * OBSERVATION_SIZE;
            let target = pending * OBSERVATION_SIZE;
            self.pending_observations[target..target + OBSERVATION_SIZE]
                .copy_from_slice(&self.batch.observations[source..source + OBSERVATION_SIZE]);
            self.pending_actions[pending] = self.batch.actions[environment];
            self.pending_rewards[pending] = 0.0;
            self.pending_distances[pending] = 0;
            self.pending_episodes[pending] = self.batch.episodes[environment];
            self.pending_seeds[pending] = self.batch.seeds[environment];
        }
        let result_count = self.batch.step()?.len();
        for result_index in 0..result_count {
            let result = self.batch.results[result_index].clone();
            for seat in 0..2 {
                let pending = result.environment * 2 + seat;
                if self.pending_valid[pending] {
                    self.pending_rewards[pending] += result.rewards[seat];
                    self.pending_distances[pending] =
                        self.pending_distances[pending].saturating_add(1);
                }
            }
            if result.terminal {
                let terminal_observation = [0.0; OBSERVATION_SIZE];
                for seat in 0..2 {
                    let pending = result.environment * 2 + seat;
                    if self.pending_valid[pending] {
                        self.complete_pending(
                            result.environment,
                            seat,
                            &terminal_observation,
                            true,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn complete_pending(
        &mut self,
        environment: usize,
        seat: usize,
        next_observation: &[f32],
        terminal: bool,
    ) -> Result<(), RlEnvironmentError> {
        if self.transitions.len == self.transitions.capacity {
            return Err(RlEnvironmentError::TransitionBufferFull);
        }
        let pending = environment * 2 + seat;
        let row = self.transitions.len;
        let source = pending * OBSERVATION_SIZE;
        let target = row * OBSERVATION_SIZE;
        self.transitions.observations[target..target + OBSERVATION_SIZE]
            .copy_from_slice(&self.pending_observations[source..source + OBSERVATION_SIZE]);
        self.transitions.next_observations[target..target + OBSERVATION_SIZE]
            .copy_from_slice(next_observation);
        self.transitions.actions[row] = self.pending_actions[pending];
        self.transitions.rewards[row] = self.pending_rewards[pending];
        self.transitions.terminals[row] = terminal;
        self.transitions.decision_time_distances[row] = self.pending_distances[pending];
        self.transitions.seats[row] = u8::try_from(seat).expect("two-player seat fits u8");
        self.transitions.environments[row] = environment;
        self.transitions.episodes[row] = self.pending_episodes[pending];
        self.transitions.seeds[row] = self.pending_seeds[pending];
        self.transitions.len += 1;
        self.pending_valid[pending] = false;
        Ok(())
    }
}

/// Wall-time diagnostic for an explicitly sized preallocated batch.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchBenchmark {
    pub batch_size: usize,
    pub decisions: u64,
    pub elapsed: Duration,
    pub decisions_per_second: f64,
    pub hot_buffer_reallocations: u64,
    pub observation_buffer_bytes: usize,
    pub mask_buffer_bytes: usize,
}

/// Comparative measurement of the four explicitly requested execution paths.
#[derive(Clone, Debug, PartialEq)]
pub struct RolloutPerformanceProfile {
    pub scalar: BatchBenchmark,
    pub naive_batch: BatchBenchmark,
    pub preallocated_batch: BatchBenchmark,
    pub parallel_batch: BatchBenchmark,
    pub parallel_workers: usize,
}

/// Measure the preallocated typed batch for at least `decisions` decisions.
///
/// # Errors
///
/// Returns a typed environment/reset/action failure.
pub fn benchmark_preallocated_batch(
    batch_size: usize,
    decisions: u64,
    seed: u64,
) -> Result<BatchBenchmark, RlEnvironmentError> {
    let mut batch = PocheBatch::new(batch_size, seed)?;
    let start = Instant::now();
    let mut completed = 0_u64;
    while completed < decisions {
        batch.select_first_legal();
        batch.step()?;
        completed = completed.saturating_add(u64::try_from(batch_size).unwrap_or(u64::MAX));
    }
    let elapsed = start.elapsed();
    let measured = completed;
    let measured_for_rate = u32::try_from(measured).unwrap_or(u32::MAX);
    Ok(BatchBenchmark {
        batch_size,
        decisions: measured,
        elapsed,
        decisions_per_second: f64::from(measured_for_rate) / elapsed.as_secs_f64(),
        hot_buffer_reallocations: batch.hot_buffer_reallocations(),
        observation_buffer_bytes: batch.observations.len() * size_of::<f32>(),
        mask_buffer_bytes: batch.legal_masks.len() * size_of::<bool>(),
    })
}

/// Measure direct typed scalar, allocating naive batch, preallocated batch,
/// and thread-partitioned preallocated batch paths.
///
/// # Errors
///
/// Returns a typed environment/reset/action failure or an invalid zero size.
pub fn benchmark_rollout_paths(
    batch_size: usize,
    decisions: u64,
    seed: u64,
) -> Result<RolloutPerformanceProfile, RlEnvironmentError> {
    if batch_size == 0 || decisions == 0 {
        return Err(RlEnvironmentError::NoAgentTurn);
    }
    let scalar = benchmark_scalar(decisions, seed)?;
    let naive_batch = benchmark_naive_batch(batch_size, decisions, seed)?;
    let preallocated_batch = benchmark_preallocated_batch(batch_size, decisions, seed)?;
    let available = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let workers = available.min(4).min(batch_size);
    let worker_batch = batch_size.div_ceil(workers);
    let worker_decisions = decisions.div_ceil(u64::try_from(workers).unwrap_or(1));
    let start = Instant::now();
    let reports = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    benchmark_preallocated_batch(
                        worker_batch,
                        worker_decisions,
                        seed ^ u64::try_from(worker).unwrap_or(u64::MAX),
                    )
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| RlEnvironmentError::NoAgentTurn)?)
            .collect::<Result<Vec<_>, _>>()
    })?;
    let elapsed = start.elapsed();
    let completed = reports.iter().map(|report| report.decisions).sum::<u64>();
    let completed_rate = u32::try_from(completed).unwrap_or(u32::MAX);
    let parallel_batch = BatchBenchmark {
        batch_size: worker_batch * workers,
        decisions: completed,
        elapsed,
        decisions_per_second: f64::from(completed_rate) / elapsed.as_secs_f64(),
        hot_buffer_reallocations: reports
            .iter()
            .map(|report| report.hot_buffer_reallocations)
            .sum(),
        observation_buffer_bytes: reports
            .iter()
            .map(|report| report.observation_buffer_bytes)
            .sum(),
        mask_buffer_bytes: reports.iter().map(|report| report.mask_buffer_bytes).sum(),
    };
    Ok(RolloutPerformanceProfile {
        scalar,
        naive_batch,
        preallocated_batch,
        parallel_batch,
        parallel_workers: workers,
    })
}

fn benchmark_scalar(decisions: u64, root_seed: u64) -> Result<BatchBenchmark, RlEnvironmentError> {
    let mut episode = 0_u64;
    let mut env = PocheRlEnv::reset(derive_episode_seed(root_seed, 0, episode))?;
    let start = Instant::now();
    for _ in 0..decisions {
        let decision = env.decision().ok_or(RlEnvironmentError::NoAgentTurn)?;
        let action = decision
            .legal_mask
            .iter()
            .position(|allowed| *allowed)
            .ok_or(RlEnvironmentError::NoAgentTurn)?;
        if env.step(ActionIndex::new(action)?)?.terminal {
            episode = episode.saturating_add(1);
            env = PocheRlEnv::reset(derive_episode_seed(root_seed, 0, episode))?;
        }
    }
    let elapsed = start.elapsed();
    let rate_count = u32::try_from(decisions).unwrap_or(u32::MAX);
    Ok(BatchBenchmark {
        batch_size: 1,
        decisions,
        elapsed,
        decisions_per_second: f64::from(rate_count) / elapsed.as_secs_f64(),
        hot_buffer_reallocations: 0,
        observation_buffer_bytes: OBSERVATION_SIZE * size_of::<f32>(),
        mask_buffer_bytes: ACTION_COUNT * size_of::<bool>(),
    })
}

fn benchmark_naive_batch(
    batch_size: usize,
    decisions: u64,
    root_seed: u64,
) -> Result<BatchBenchmark, RlEnvironmentError> {
    let mut episodes = vec![0_u64; batch_size];
    let mut environments = (0..batch_size)
        .map(|index| PocheRlEnv::reset(derive_episode_seed(root_seed, index, 0)))
        .collect::<Result<Vec<_>, _>>()?;
    let start = Instant::now();
    let mut completed = 0_u64;
    while completed < decisions {
        let actions = environments
            .iter()
            .map(|env| {
                let decision = env.decision().ok_or(RlEnvironmentError::NoAgentTurn)?;
                decision
                    .legal_mask
                    .iter()
                    .position(|allowed| *allowed)
                    .ok_or(RlEnvironmentError::NoAgentTurn)
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, (env, action)) in environments.iter_mut().zip(actions).enumerate() {
            if env.step(ActionIndex::new(action)?)?.terminal {
                episodes[index] = episodes[index].saturating_add(1);
                *env = PocheRlEnv::reset(derive_episode_seed(root_seed, index, episodes[index]))?;
            }
        }
        completed = completed.saturating_add(u64::try_from(batch_size).unwrap_or(u64::MAX));
    }
    let elapsed = start.elapsed();
    let rate_count = u32::try_from(completed).unwrap_or(u32::MAX);
    Ok(BatchBenchmark {
        batch_size,
        decisions: completed,
        elapsed,
        decisions_per_second: f64::from(rate_count) / elapsed.as_secs_f64(),
        hot_buffer_reallocations: completed.div_ceil(u64::try_from(batch_size).unwrap_or(1)),
        observation_buffer_bytes: batch_size * OBSERVATION_SIZE * size_of::<f32>(),
        mask_buffer_bytes: batch_size * ACTION_COUNT * size_of::<bool>(),
    })
}

/// Produce deterministic per-seed rollout hashes sequentially or in parallel;
/// this is the semantic parity gate for thread partitioning.
///
/// # Errors
///
/// Returns a typed environment/action failure.
pub fn rollout_signatures(
    seeds: &[u64],
    parallel: bool,
) -> Result<Vec<String>, RlEnvironmentError> {
    if parallel {
        return std::thread::scope(|scope| {
            let handles = seeds
                .iter()
                .map(|seed| scope.spawn(move || rollout_signature(*seed)))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().map_err(|_| RlEnvironmentError::NoAgentTurn)?)
                .collect()
        });
    }
    seeds.iter().map(|seed| rollout_signature(*seed)).collect()
}

fn rollout_signature(seed: u64) -> Result<String, RlEnvironmentError> {
    let mut env = PocheRlEnv::reset(seed)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-rollout-signature-v1\0");
    while let Some(decision) = env.decision() {
        hasher.update(decision.observation_hash.as_bytes());
        let action = decision
            .legal_mask
            .iter()
            .position(|allowed| *allowed)
            .ok_or(RlEnvironmentError::NoAgentTurn)?;
        hasher.update(&u64::try_from(action).unwrap_or(u64::MAX).to_le_bytes());
        let step = env.step(ActionIndex::new(action)?)?;
        for reward in step.instant_rewards {
            hasher.update(&reward.to_le_bytes());
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn scalar_and_preallocated_batch_agree_step_for_step() {
        let root = 77;
        let mut scalar = PocheRlEnv::reset(derive_episode_seed(root, 0, 0)).unwrap();
        let mut batch = PocheBatch::new(1, root).unwrap();
        for _ in 0..1_000 {
            let scalar_decision = scalar.decision().unwrap();
            let scalar_action = scalar_decision
                .legal_mask
                .iter()
                .position(|allowed| *allowed)
                .unwrap();
            assert_eq!(&scalar_decision.observation, batch.observations());
            assert_eq!(&scalar_decision.legal_mask, batch.legal_masks());
            batch.select_first_legal();
            let batch_step = batch.step().unwrap()[0].clone();
            let scalar_step = scalar
                .step(ActionIndex::new(scalar_action).unwrap())
                .unwrap();
            assert_eq!(batch_step.rewards, scalar_step.instant_rewards);
            assert_eq!(batch_step.terminal, scalar_step.terminal);
            if scalar_step.terminal {
                break;
            }
        }
        assert_eq!(batch.hot_buffer_reallocations(), 0);
    }

    #[test]
    fn deterministic_seed_derivation_separates_slots_and_episodes() {
        assert_eq!(derive_episode_seed(1, 2, 3), derive_episode_seed(1, 2, 3));
        assert_ne!(derive_episode_seed(1, 2, 3), derive_episode_seed(1, 3, 3));
        assert_ne!(derive_episode_seed(1, 2, 3), derive_episode_seed(1, 2, 4));
    }

    #[test]
    fn smaller_diagnostic_reports_preallocated_buffers() {
        let report = benchmark_preallocated_batch(8, 1_024, 4).unwrap();
        assert!(report.decisions >= 1_024);
        assert!(report.decisions_per_second.is_finite());
        assert_eq!(report.hot_buffer_reallocations, 0);
        assert_eq!(report.observation_buffer_bytes, 8 * OBSERVATION_SIZE * 4);
        assert_eq!(report.mask_buffer_bytes, 8 * ACTION_COUNT);
    }

    #[test]
    fn thread_partitioning_retains_exact_scalar_semantics() {
        let seeds = [1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(
            rollout_signatures(&seeds, false).unwrap(),
            rollout_signatures(&seeds, true).unwrap()
        );
    }

    #[test]
    fn turn_based_batch_pairs_each_action_with_that_seats_next_observation() {
        let mut rollout = TurnBasedBatchRollout::new(1, 0x5eed, 256).unwrap();
        while rollout.batch().episodes()[0] == 0 {
            rollout.step_first_legal().unwrap();
        }
        let transitions = rollout.transitions();
        assert_eq!(transitions.len(), 124);
        assert_eq!(
            transitions
                .terminals()
                .iter()
                .filter(|terminal| **terminal)
                .count(),
            2
        );
        assert!(
            transitions
                .decision_time_distances()
                .iter()
                .all(|distance| *distance > 0)
        );
        for row in 0..transitions.len() {
            let next = &transitions.next_observations()
                [row * OBSERVATION_SIZE..(row + 1) * OBSERVATION_SIZE];
            if transitions.terminals()[row] {
                assert!(next.iter().all(|value| *value == 0.0));
            } else {
                assert_eq!(next[11], 1.0, "next observation actor is the same seat");
            }
        }
        assert!(transitions.rewards().iter().any(|reward| *reward > 0.0));
        assert!(
            transitions
                .seeds()
                .iter()
                .all(|seed| *seed == transitions.seeds()[0])
        );
    }
}

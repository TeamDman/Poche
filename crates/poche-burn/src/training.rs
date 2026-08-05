// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use burn::{
    backend::{Flex, flex::FlexDevice},
    prelude::*,
    tensor::TensorData,
};
use poche_rl::{
    ACTION_COUNT, ActionIndex, DecisionView, EpisodeTranscript, HighCardHeuristicPolicy,
    LegalRandomPolicy, OBSERVATION_SIZE, PocheRlEnv, REWARD_ID, ReplayRng, RlSpec, SPEC_ID,
    run_episode_with_seat_policies,
};
use serde::{Deserialize, Serialize};

use crate::{
    ActorCritic, ActorCriticConfig, CheckpointManifest, CpuBackend, FrozenPolicy, FrozenPolicyPool,
    GaeInput, GreedyBurnPolicy, PpoBatch, PpoConfig, PpoLearner, PpoMetrics, digest,
    generalized_advantage_estimate, masked_probabilities,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingRunManifest {
    pub schema_version: u16,
    pub run_id: String,
    pub burn_version: String,
    pub backend: String,
    pub spec_id: String,
    pub spec_hash: String,
    pub reward_id: String,
    pub model: ActorCriticConfig,
    pub ppo: PpoConfig,
    pub root_seed: u64,
    pub updates: usize,
    pub episodes_per_update: usize,
    pub self_play_pool_capacity: usize,
    pub held_out_seeds: Vec<u64>,
    pub artifact_directory: String,
    pub empirical_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingError {
    Io,
    Manifest,
    Environment,
    Learner,
    Record,
    Evaluation,
}

impl TrainingRunManifest {
    /// Strictly load and validate a committed run manifest.
    ///
    /// # Errors
    /// Rejects IO, JSON, semantic drift, empty scopes, or unsafe artifact paths.
    pub fn load(path: &Path) -> Result<Self, TrainingError> {
        let bytes = std::fs::read(path).map_err(|_| TrainingError::Io)?;
        let manifest: Self = serde_json::from_slice(&bytes).map_err(|_| TrainingError::Manifest)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate immutable environment and algorithm bindings.
    ///
    /// # Errors
    /// Returns [`TrainingError::Manifest`] for drift.
    pub fn validate(&self) -> Result<(), TrainingError> {
        let expected_hash = RlSpec::poche_2p_v1()
            .semantic_hash()
            .map_err(|_| TrainingError::Manifest)?;
        let artifact = Path::new(&self.artifact_directory);
        if self.schema_version != 1
            || self.burn_version != "0.21.0"
            || self.backend != "flex-cpu"
            || self.spec_id != SPEC_ID
            || self.spec_hash != expected_hash
            || self.reward_id != REWARD_ID
            || self.model.observation_size != OBSERVATION_SIZE
            || self.model.action_count != ACTION_COUNT
            || self.model.hidden_size == 0
            || self.updates == 0
            || self.episodes_per_update < 2
            || self.self_play_pool_capacity < self.updates + 1
            || self.held_out_seeds.is_empty()
            || !self.empirical_only
            || artifact.is_absolute()
            || !artifact.starts_with("artifacts/rl")
        {
            return Err(TrainingError::Manifest);
        }
        Ok(())
    }

    /// Canonical manifest hash used by run evidence.
    ///
    /// # Errors
    /// Returns a stable manifest error for serialization failure.
    pub fn semantic_hash(&self) -> Result<String, TrainingError> {
        serde_json::to_vec(self)
            .map(|bytes| digest(&bytes))
            .map_err(|_| TrainingError::Manifest)
    }

    #[must_use]
    pub fn artifact_path(&self) -> PathBuf {
        PathBuf::from(&self.artifact_directory)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSummary {
    pub update: usize,
    pub episodes: usize,
    pub transition_rows: usize,
    pub mean_total_loss: f64,
    pub mean_policy_loss: f64,
    pub mean_value_loss: f64,
    pub mean_entropy: f64,
    pub rollout_matchups: Vec<[String; 2]>,
    pub frozen_checkpoint_digest: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingSummary {
    pub schema_version: u16,
    pub run_id: String,
    pub manifest_hash: String,
    pub burn_version: String,
    pub backend: String,
    pub episodes: usize,
    pub transition_rows: usize,
    pub update_summaries: Vec<UpdateSummary>,
    pub final_checkpoint: CheckpointManifest,
    pub frozen_policies: Vec<FrozenPolicy>,
    pub empirical_only: bool,
}

#[derive(Clone, Debug)]
struct Pending {
    observation: [f32; OBSERVATION_SIZE],
    legal_mask: [bool; ACTION_COUNT],
    action: usize,
    old_log_probability: f32,
    value: f32,
    reward: f32,
    distance: u32,
}

#[derive(Clone, Debug)]
struct RawRow {
    seat: usize,
    observation: [f32; OBSERVATION_SIZE],
    legal_mask: [bool; ACTION_COUNT],
    action: usize,
    old_log_probability: f32,
    reward: f32,
    value: f32,
    next_value: f32,
    terminal: bool,
    distance: u32,
}

fn infer(model: &ActorCritic<Flex>, decision: &DecisionView) -> (Vec<f32>, f32) {
    let output = model.forward(Tensor::from_data(
        TensorData::new(decision.observation.to_vec(), [1, OBSERVATION_SIZE]),
        &FlexDevice,
    ));
    let logits = output
        .logits
        .into_data()
        .to_vec::<f32>()
        .expect("Flex f32 logits");
    let value = output.values.into_scalar();
    (logits, value)
}

/// Hash fixed viewer-only policy/value outputs independently of Burn's random
/// internal parameter IDs in serialized record containers.
fn policy_probe_digest(model: &ActorCritic<Flex>) -> Result<String, TrainingError> {
    let mut bytes = Vec::new();
    for seed in [0_u64, 1, 0x5eed, 0xb453_0010] {
        let decision = PocheRlEnv::reset(seed)
            .map_err(|_| TrainingError::Environment)?
            .decision()
            .ok_or(TrainingError::Environment)?;
        let (logits, value) = infer(model, &decision);
        for logit in logits {
            bytes.extend_from_slice(&logit.to_le_bytes());
        }
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(digest(&bytes))
}

fn sampled_action(probabilities: &[f32], rng: &mut ReplayRng) -> usize {
    let high = u32::try_from(rng.next_u64() >> 32).expect("shifted random value fits u32");
    let unit = f64::from(high) / (f64::from(u32::MAX) + 1.0);
    let mut cumulative = 0.0_f64;
    for (index, probability) in probabilities.iter().enumerate() {
        cumulative += f64::from(*probability);
        if unit < cumulative {
            return index;
        }
    }
    probabilities
        .iter()
        .rposition(|probability| *probability > 0.0)
        .expect("masked distribution has a legal action")
}

#[allow(clippy::too_many_lines)] // Mirrors the same-seat pending transition contract in one auditable loop.
fn collect_episode(
    seed: u64,
    current: &ActorCritic<Flex>,
    opponent: &ActorCritic<Flex>,
    current_seats: [bool; 2],
    ppo: PpoConfig,
) -> Result<PpoBatch, TrainingError> {
    let mut env = PocheRlEnv::reset(seed).map_err(|_| TrainingError::Environment)?;
    let mut rng = ReplayRng::new(seed ^ 0x5050_4f43_4845);
    let mut pending: [Option<Pending>; 2] = [None, None];
    let mut rows = Vec::new();
    loop {
        let decision = env.decision().ok_or(TrainingError::Environment)?;
        let seat = decision.seat.index();
        let model = if current_seats[seat] {
            current
        } else {
            opponent
        };
        let (logits, next_value) = infer(model, &decision);
        if let Some(completed) = pending[seat].take() {
            rows.push(RawRow {
                seat,
                observation: completed.observation,
                legal_mask: completed.legal_mask,
                action: completed.action,
                old_log_probability: completed.old_log_probability,
                reward: completed.reward,
                value: completed.value,
                next_value,
                terminal: false,
                distance: completed.distance,
            });
        }
        let probabilities = masked_probabilities(&logits, &decision.legal_mask);
        let action = if current_seats[seat] {
            sampled_action(&probabilities, &mut rng)
        } else {
            (0..ACTION_COUNT)
                .filter(|index| decision.legal_mask[*index])
                .max_by(|left, right| logits[*left].total_cmp(&logits[*right]))
                .ok_or(TrainingError::Environment)?
        };
        if current_seats[seat] {
            pending[seat] = Some(Pending {
                observation: decision.observation,
                legal_mask: decision.legal_mask,
                action,
                old_log_probability: probabilities[action].ln(),
                value: next_value,
                reward: 0.0,
                distance: 0,
            });
        }
        let step = env
            .step(ActionIndex::new(action).map_err(|_| TrainingError::Environment)?)
            .map_err(|_| TrainingError::Environment)?;
        for (index, item) in pending.iter_mut().enumerate() {
            if let Some(item) = item {
                item.reward += step.instant_rewards[index];
                item.distance = item.distance.saturating_add(1);
            }
        }
        if step.terminal {
            for (seat, item) in pending.iter_mut().enumerate() {
                if let Some(completed) = item.take() {
                    rows.push(RawRow {
                        seat,
                        observation: completed.observation,
                        legal_mask: completed.legal_mask,
                        action: completed.action,
                        old_log_probability: completed.old_log_probability,
                        reward: completed.reward,
                        value: completed.value,
                        next_value: 0.0,
                        terminal: true,
                        distance: completed.distance,
                    });
                }
            }
            break;
        }
    }
    let mut output = PpoBatch {
        observations: Vec::new(),
        legal_masks: Vec::new(),
        actions: Vec::new(),
        old_log_probabilities: Vec::new(),
        advantages: Vec::new(),
        returns: Vec::new(),
    };
    for seat in 0..2 {
        let seat_rows = rows
            .iter()
            .filter(|row| row.seat == seat)
            .collect::<Vec<_>>();
        let gae = generalized_advantage_estimate(
            &seat_rows
                .iter()
                .map(|row| GaeInput {
                    reward: row.reward,
                    value: row.value,
                    next_value: row.next_value,
                    terminal: row.terminal,
                    decision_time_distance: row.distance,
                })
                .collect::<Vec<_>>(),
            ppo,
        );
        for (index, row) in seat_rows.into_iter().enumerate() {
            output.observations.extend_from_slice(&row.observation);
            output.legal_masks.extend_from_slice(&row.legal_mask);
            output.actions.push(row.action);
            output.old_log_probabilities.push(row.old_log_probability);
            output.advantages.push(gae.advantages[index]);
            output.returns.push(gae.returns[index]);
        }
    }
    output.validate().map_err(|_| TrainingError::Learner)?;
    Ok(output)
}

fn append_batch(target: &mut PpoBatch, source: PpoBatch) {
    target.observations.extend(source.observations);
    target.legal_masks.extend(source.legal_masks);
    target.actions.extend(source.actions);
    target
        .old_log_probabilities
        .extend(source.old_log_probabilities);
    target.advantages.extend(source.advantages);
    target.returns.extend(source.returns);
}

fn empty_batch() -> PpoBatch {
    PpoBatch {
        observations: Vec::new(),
        legal_masks: Vec::new(),
        actions: Vec::new(),
        old_log_probabilities: Vec::new(),
        advantages: Vec::new(),
        returns: Vec::new(),
    }
}

fn shuffle(indices: &mut [usize], rng: &mut ReplayRng) {
    for index in (1..indices.len()).rev() {
        let upper = u64::try_from(index + 1).unwrap_or(u64::MAX);
        let swap = usize::try_from(rng.next_u64() % upper).unwrap_or(0);
        indices.swap(index, swap);
    }
}

/// Run the preregistered short full-rule CPU experiment and write large
/// records only beneath its ignored artifact directory.
///
/// # Errors
/// Returns a stable manifest/IO/environment/learner category.
#[allow(clippy::too_many_lines)] // Keeps one run's artifact transaction and evidence assembly together.
pub fn train(manifest: &TrainingRunManifest) -> Result<TrainingSummary, TrainingError> {
    manifest.validate()?;
    let artifact = manifest.artifact_path();
    std::fs::create_dir_all(&artifact).map_err(|_| TrainingError::Io)?;
    let device = FlexDevice;
    CpuBackend::seed(&device, manifest.root_seed);
    let mut learner = PpoLearner::<CpuBackend>::new(manifest.model, manifest.ppo, &device);
    // Materialize Burn's lazy parameters before the initial immutable record.
    let _ = learner.model.forward(Tensor::from_data(
        TensorData::new(vec![0.0; OBSERVATION_SIZE], [1, OBSERVATION_SIZE]),
        &device,
    ));
    let mut frozen_pool = FrozenPolicyPool::new(manifest.self_play_pool_capacity);
    let mut frozen_records = Vec::new();
    let (initial_model, initial_optimizer) =
        learner.record_bytes().map_err(|_| TrainingError::Record)?;
    let initial = FrozenPolicy {
        policy_id: "poche-ppo-v1-update-0".to_owned(),
        checkpoint_digest: digest(&initial_model),
        update: 0,
    };
    frozen_pool
        .freeze(initial.clone())
        .map_err(|_| TrainingError::Record)?;
    frozen_records.push((initial, initial_model, initial_optimizer));
    let mut update_summaries = Vec::new();
    let mut total_rows = 0;

    for update in 0..manifest.updates {
        let current = learner.valid();
        let frozen_index = update % frozen_records.len();
        let frozen_record = &frozen_records[frozen_index];
        let opponent = PpoLearner::<CpuBackend>::new(manifest.model, manifest.ppo, &device)
            .load_record_bytes(frozen_record.1.clone(), frozen_record.2.clone(), &device)
            .map_err(|_| TrainingError::Record)?
            .valid();
        let mut rollout = empty_batch();
        let mut matchups = Vec::new();
        for episode in 0..manifest.episodes_per_update {
            let seed = manifest.root_seed
                ^ u64::try_from(update)
                    .unwrap_or(u64::MAX)
                    .wrapping_mul(0x9e37_79b9)
                ^ u64::try_from(episode).unwrap_or(u64::MAX);
            let current_seats = if episode == 0 {
                [true, true]
            } else if seed & 1 == 0 {
                [true, false]
            } else {
                [false, true]
            };
            matchups.push(if current_seats == [true, true] {
                ["current".to_owned(), "current".to_owned()]
            } else if current_seats[0] {
                ["current".to_owned(), frozen_record.0.policy_id.clone()]
            } else {
                [frozen_record.0.policy_id.clone(), "current".to_owned()]
            });
            append_batch(
                &mut rollout,
                collect_episode(seed, &current, &opponent, current_seats, manifest.ppo)?,
            );
        }
        let rows = rollout.validate().map_err(|_| TrainingError::Learner)?;
        total_rows += rows;
        let mut metrics = Vec::new();
        let mut order = (0..rows).collect::<Vec<_>>();
        let mut rng = ReplayRng::new(manifest.root_seed ^ update as u64);
        for _ in 0..manifest.ppo.epochs {
            shuffle(&mut order, &mut rng);
            for chunk in order.chunks(manifest.ppo.minibatch_size) {
                let batch = rollout
                    .select_rows(chunk)
                    .map_err(|_| TrainingError::Learner)?;
                metrics.push(
                    learner
                        .update(&batch, &device)
                        .map_err(|_| TrainingError::Learner)?,
                );
            }
        }
        let (model_bytes, optimizer_bytes) =
            learner.record_bytes().map_err(|_| TrainingError::Record)?;
        let frozen = FrozenPolicy {
            policy_id: format!("poche-ppo-v1-update-{}", update + 1),
            checkpoint_digest: digest(&model_bytes),
            update: u64::try_from(update + 1).unwrap_or(u64::MAX),
        };
        frozen_pool
            .freeze(frozen.clone())
            .map_err(|_| TrainingError::Record)?;
        frozen_records.push((frozen.clone(), model_bytes, optimizer_bytes));
        let mean = |field: fn(&PpoMetrics) -> f32| {
            let count = u32::try_from(metrics.len()).unwrap_or(u32::MAX);
            metrics.iter().map(field).map(f64::from).sum::<f64>() / f64::from(count)
        };
        update_summaries.push(UpdateSummary {
            update: update + 1,
            episodes: manifest.episodes_per_update,
            transition_rows: rows,
            mean_total_loss: mean(|item| item.total_loss),
            mean_policy_loss: mean(|item| item.policy_loss),
            mean_value_loss: mean(|item| item.value_loss),
            mean_entropy: mean(|item| item.entropy),
            rollout_matchups: matchups,
            frozen_checkpoint_digest: frozen.checkpoint_digest,
        });
    }
    let (model_bytes, optimizer_bytes) =
        learner.record_bytes().map_err(|_| TrainingError::Record)?;
    let policy_probe_digest = policy_probe_digest(&learner.valid())?;
    let checkpoint = CheckpointManifest {
        schema_version: 1,
        run_id: manifest.run_id.clone(),
        burn_version: manifest.burn_version.clone(),
        spec_id: manifest.spec_id.clone(),
        spec_hash: manifest.spec_hash.clone(),
        reward_id: manifest.reward_id.clone(),
        model: manifest.model,
        ppo: manifest.ppo,
        seed: manifest.root_seed,
        updates: u64::try_from(manifest.updates).unwrap_or(u64::MAX),
        model_digest: digest(&model_bytes),
        optimizer_digest: digest(&optimizer_bytes),
        policy_probe_digest,
    };
    let summary = TrainingSummary {
        schema_version: 1,
        run_id: manifest.run_id.clone(),
        manifest_hash: manifest.semantic_hash()?,
        burn_version: manifest.burn_version.clone(),
        backend: manifest.backend.clone(),
        episodes: manifest.updates * manifest.episodes_per_update,
        transition_rows: total_rows,
        update_summaries,
        final_checkpoint: checkpoint.clone(),
        frozen_policies: frozen_pool.policies().to_vec(),
        empirical_only: true,
    };
    std::fs::write(artifact.join("model.bin"), &model_bytes).map_err(|_| TrainingError::Io)?;
    std::fs::write(artifact.join("optimizer.bin"), &optimizer_bytes)
        .map_err(|_| TrainingError::Io)?;
    std::fs::write(
        artifact.join("checkpoint.json"),
        checkpoint
            .canonical_json()
            .map_err(|_| TrainingError::Record)?,
    )
    .map_err(|_| TrainingError::Io)?;
    std::fs::write(
        artifact.join("training-summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|_| TrainingError::Manifest)?,
    )
    .map_err(|_| TrainingError::Io)?;
    Ok(summary)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedMatchupSummary {
    pub name: String,
    pub seat_policies: [String; 2],
    pub episodes: usize,
    pub mean_scores: [f64; 2],
    pub mean_score_differential_seat0: f64,
    pub differential_ci95: [f64; 2],
    pub mean_round_scores: Vec<[f64; 2]>,
    pub mean_exact_bids: [f64; 2],
    pub mean_game_length: f64,
    pub wins: [usize; 2],
    pub ties: usize,
    pub illegal_actions: u64,
    pub selected: Vec<SelectedEpisode>,
    pub failure_episode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedEpisode {
    pub label: String,
    pub seed: u64,
    pub final_scores: [u16; 2],
    pub differential_seat0: i32,
    pub semantic_hash: String,
    pub ndjson_path: String,
    pub json_path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnedEvaluationSummary {
    pub schema_version: u16,
    pub run_id: String,
    pub manifest_hash: String,
    pub checkpoint_digest: String,
    pub corpus_seeds: Vec<u64>,
    pub matchups: Vec<LearnedMatchupSummary>,
    pub policy_quality: String,
    pub hidden_state_exposed: bool,
    pub empirical_only: bool,
}

fn write_selected_episodes(
    name: &str,
    episodes: &[EpisodeTranscript],
    replay_directory: &Path,
) -> Result<Vec<SelectedEpisode>, TrainingError> {
    let indices = [0, (episodes.len() - 1) / 2, episodes.len() - 1];
    let labels = ["worst", "median", "best"];
    labels
        .into_iter()
        .zip(indices)
        .map(|(label, index)| {
            let episode = &episodes[index];
            let stem = format!("{name}-{label}-{}", episode.seed);
            let ndjson_path = replay_directory.join(format!("{stem}.ndjson"));
            let json_path = replay_directory.join(format!("{stem}.json"));
            std::fs::write(
                &ndjson_path,
                episode.ndjson().map_err(|_| TrainingError::Evaluation)?,
            )
            .map_err(|_| TrainingError::Io)?;
            std::fs::write(
                &json_path,
                episode
                    .canonical_json()
                    .map_err(|_| TrainingError::Evaluation)?,
            )
            .map_err(|_| TrainingError::Io)?;
            Ok(SelectedEpisode {
                label: label.to_owned(),
                seed: episode.seed,
                final_scores: episode.final_scores,
                differential_seat0: episode.score_differential_seat0,
                semantic_hash: episode
                    .semantic_hash()
                    .map_err(|_| TrainingError::Evaluation)?,
                ndjson_path: ndjson_path.to_string_lossy().replace('\\', "/"),
                json_path: json_path.to_string_lossy().replace('\\', "/"),
            })
        })
        .collect()
}

fn summarize_matchup(
    name: &str,
    mut episodes: Vec<EpisodeTranscript>,
    replay_directory: &Path,
) -> Result<LearnedMatchupSummary, TrainingError> {
    episodes.sort_by_key(|episode| (episode.score_differential_seat0, episode.seed));
    let count = f64::from(u32::try_from(episodes.len()).unwrap_or(u32::MAX));
    let mean0 = episodes
        .iter()
        .map(|episode| f64::from(episode.final_scores[0]))
        .sum::<f64>()
        / count;
    let mean1 = episodes
        .iter()
        .map(|episode| f64::from(episode.final_scores[1]))
        .sum::<f64>()
        / count;
    let mean_diff = episodes
        .iter()
        .map(|episode| f64::from(episode.score_differential_seat0))
        .sum::<f64>()
        / count;
    let variance = episodes
        .iter()
        .map(|episode| (f64::from(episode.score_differential_seat0) - mean_diff).powi(2))
        .sum::<f64>()
        / (count - 1.0).max(1.0);
    let margin = 1.96 * (variance / count).sqrt();
    let selected = write_selected_episodes(name, &episodes, replay_directory)?;
    let failures = episodes
        .iter()
        .filter(|episode| episode.illegal_action_count > 0)
        .collect::<Vec<_>>();
    Ok(LearnedMatchupSummary {
        name: name.to_owned(),
        seat_policies: episodes[0].seat_policies.clone(),
        episodes: episodes.len(),
        mean_scores: [mean0, mean1],
        mean_score_differential_seat0: mean_diff,
        differential_ci95: [mean_diff - margin, mean_diff + margin],
        mean_round_scores: (0..13)
            .map(|round| {
                [
                    episodes
                        .iter()
                        .map(|episode| f64::from(episode.round_points[round][0]))
                        .sum::<f64>()
                        / count,
                    episodes
                        .iter()
                        .map(|episode| f64::from(episode.round_points[round][1]))
                        .sum::<f64>()
                        / count,
                ]
            })
            .collect(),
        mean_exact_bids: [
            episodes
                .iter()
                .map(|episode| f64::from(episode.exact_bid_count[0]))
                .sum::<f64>()
                / count,
            episodes
                .iter()
                .map(|episode| f64::from(episode.exact_bid_count[1]))
                .sum::<f64>()
                / count,
        ],
        mean_game_length: episodes
            .iter()
            .map(|episode| {
                f64::from(u32::try_from(episode.game_length_decisions).unwrap_or(u32::MAX))
            })
            .sum::<f64>()
            / count,
        wins: [
            episodes
                .iter()
                .filter(|episode| episode.winners[0] && !episode.winners[1])
                .count(),
            episodes
                .iter()
                .filter(|episode| episode.winners[1] && !episode.winners[0])
                .count(),
        ],
        ties: episodes
            .iter()
            .filter(|episode| episode.winners == [true, true])
            .count(),
        illegal_actions: episodes
            .iter()
            .map(|episode| episode.illegal_action_count)
            .sum(),
        failure_episode: failures
            .first()
            .and_then(|episode| episode.semantic_hash().ok()),
        selected,
    })
}

/// Evaluate the saved policy against fixed held-out baselines with seat swaps.
///
/// # Errors
/// Rejects missing/mismatched artifacts or an episode/replay failure.
pub fn evaluate(manifest: &TrainingRunManifest) -> Result<LearnedEvaluationSummary, TrainingError> {
    manifest.validate()?;
    let artifact = manifest.artifact_path();
    let model_bytes = std::fs::read(artifact.join("model.bin")).map_err(|_| TrainingError::Io)?;
    let optimizer_bytes =
        std::fs::read(artifact.join("optimizer.bin")).map_err(|_| TrainingError::Io)?;
    let checkpoint: CheckpointManifest = serde_json::from_slice(
        &std::fs::read(artifact.join("checkpoint.json")).map_err(|_| TrainingError::Io)?,
    )
    .map_err(|_| TrainingError::Record)?;
    checkpoint
        .validate(
            &manifest.spec_id,
            &manifest.spec_hash,
            &manifest.reward_id,
            &model_bytes,
            &optimizer_bytes,
        )
        .map_err(|_| TrainingError::Record)?;
    let model = PpoLearner::<CpuBackend>::new(manifest.model, manifest.ppo, &FlexDevice)
        .load_record_bytes(model_bytes, optimizer_bytes, &FlexDevice)
        .map_err(|_| TrainingError::Record)?
        .valid();
    if policy_probe_digest(&model)? != checkpoint.policy_probe_digest {
        return Err(TrainingError::Record);
    }
    let replay_directory = artifact.join("replays");
    std::fs::create_dir_all(&replay_directory).map_err(|_| TrainingError::Io)?;
    let mut groups = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for seed in &manifest.held_out_seeds {
        let learned0 = &mut GreedyBurnPolicy::new(model.clone(), FlexDevice);
        let random1 = &mut LegalRandomPolicy::new(*seed ^ 0x1001);
        groups[0].push(
            run_episode_with_seat_policies(*seed, learned0, random1)
                .map_err(|_| TrainingError::Evaluation)?,
        );
        let random0 = &mut LegalRandomPolicy::new(*seed ^ 0x1002);
        let learned1 = &mut GreedyBurnPolicy::new(model.clone(), FlexDevice);
        groups[1].push(
            run_episode_with_seat_policies(*seed, random0, learned1)
                .map_err(|_| TrainingError::Evaluation)?,
        );
        let learned0 = &mut GreedyBurnPolicy::new(model.clone(), FlexDevice);
        let heuristic1 = &mut HighCardHeuristicPolicy;
        groups[2].push(
            run_episode_with_seat_policies(*seed, learned0, heuristic1)
                .map_err(|_| TrainingError::Evaluation)?,
        );
        let heuristic0 = &mut HighCardHeuristicPolicy;
        let learned1 = &mut GreedyBurnPolicy::new(model.clone(), FlexDevice);
        groups[3].push(
            run_episode_with_seat_policies(*seed, heuristic0, learned1)
                .map_err(|_| TrainingError::Evaluation)?,
        );
    }
    let names = [
        "learned-vs-legal-random",
        "legal-random-vs-learned",
        "learned-vs-heuristic",
        "heuristic-vs-learned",
    ];
    let mut matchups = Vec::new();
    for (name, episodes) in names.into_iter().zip(groups) {
        matchups.push(summarize_matchup(name, episodes, &replay_directory)?);
    }
    let summary = LearnedEvaluationSummary {
        schema_version: 1,
        run_id: manifest.run_id.clone(),
        manifest_hash: manifest.semantic_hash()?,
        checkpoint_digest: checkpoint.model_digest,
        corpus_seeds: manifest.held_out_seeds.clone(),
        matchups,
        policy_quality:
            "empirical score estimates on the disclosed held-out corpus; not formal evidence"
                .to_owned(),
        hidden_state_exposed: false,
        empirical_only: true,
    };
    std::fs::write(
        artifact.join("evaluation-summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|_| TrainingError::Evaluation)?,
    )
    .map_err(|_| TrainingError::Io)?;
    Ok(summary)
}

/// Load one evaluation-selected learned episode as a strict, secret-free replay.
///
/// # Errors
/// Rejects stale/tampered evaluation metadata, checkpoint drift, an unselected
/// seed, or episode semantic/hash drift.
pub fn replay_selected(
    manifest: &TrainingRunManifest,
    matchup_name: &str,
    seed: u64,
) -> Result<EpisodeTranscript, TrainingError> {
    manifest.validate()?;
    let artifact = manifest.artifact_path();
    let model_bytes = std::fs::read(artifact.join("model.bin")).map_err(|_| TrainingError::Io)?;
    let optimizer_bytes =
        std::fs::read(artifact.join("optimizer.bin")).map_err(|_| TrainingError::Io)?;
    let checkpoint: CheckpointManifest = serde_json::from_slice(
        &std::fs::read(artifact.join("checkpoint.json")).map_err(|_| TrainingError::Io)?,
    )
    .map_err(|_| TrainingError::Record)?;
    checkpoint
        .validate(
            &manifest.spec_id,
            &manifest.spec_hash,
            &manifest.reward_id,
            &model_bytes,
            &optimizer_bytes,
        )
        .map_err(|_| TrainingError::Record)?;
    let summary: LearnedEvaluationSummary = serde_json::from_slice(
        &std::fs::read(artifact.join("evaluation-summary.json")).map_err(|_| TrainingError::Io)?,
    )
    .map_err(|_| TrainingError::Evaluation)?;
    if summary.schema_version != 1
        || summary.run_id != manifest.run_id
        || summary.manifest_hash != manifest.semantic_hash()?
        || summary.checkpoint_digest != checkpoint.model_digest
        || summary.corpus_seeds != manifest.held_out_seeds
        || summary.hidden_state_exposed
        || !summary.empirical_only
    {
        return Err(TrainingError::Evaluation);
    }
    let matchup = summary
        .matchups
        .iter()
        .find(|matchup| matchup.name == matchup_name)
        .ok_or(TrainingError::Evaluation)?;
    let selected = matchup
        .selected
        .iter()
        .find(|selected| selected.seed == seed)
        .ok_or(TrainingError::Evaluation)?;
    if !matches!(selected.label.as_str(), "worst" | "median" | "best")
        || !matches!(
            matchup_name,
            "learned-vs-legal-random"
                | "legal-random-vs-learned"
                | "learned-vs-heuristic"
                | "heuristic-vs-learned"
        )
    {
        return Err(TrainingError::Evaluation);
    }
    let filename = format!("{matchup_name}-{}-{seed}.json", selected.label);
    let episode: EpisodeTranscript = serde_json::from_slice(
        &std::fs::read(artifact.join("replays").join(filename)).map_err(|_| TrainingError::Io)?,
    )
    .map_err(|_| TrainingError::Evaluation)?;
    let episode_hash = episode
        .semantic_hash()
        .map_err(|_| TrainingError::Evaluation)?;
    if episode.schema_version != 1
        || episode.spec_id != manifest.spec_id
        || episode.spec_hash != manifest.spec_hash
        || episode.reward_id != manifest.reward_id
        || episode.seed != seed
        || episode.seat_policies != matchup.seat_policies
        || episode.final_scores != selected.final_scores
        || episode.score_differential_seat0 != selected.differential_seat0
        || episode.illegal_action_count != 0
        || episode_hash != selected.semantic_hash
    {
        return Err(TrainingError::Evaluation);
    }
    Ok(episode)
}

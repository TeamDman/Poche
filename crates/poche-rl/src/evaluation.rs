// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::{
    EpisodeError, HighCardHeuristicPolicy, LegalRandomPolicy, Policy, RlSpec, UniformRandomPolicy,
    run_episode_with_seat_policies,
};

/// One fixed, seat-specific baseline matchup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineMatchup {
    pub name: String,
    pub seat0: String,
    pub seat1: String,
}

/// Preregistered baseline evaluation corpus created before any learning run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineCorpusManifest {
    pub schema_version: u16,
    pub corpus_id: String,
    pub spec_id: String,
    pub spec_hash: String,
    pub reward_id: String,
    pub seeds: Vec<u64>,
    pub matchups: Vec<BaselineMatchup>,
    pub selection_rule: String,
}

impl BaselineCorpusManifest {
    /// Construct the immutable first evaluation corpus: 64 held-out seeds and
    /// both seat assignments for legal-random versus heuristic, plus the two
    /// random controls.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the RL spec cannot be hashed.
    pub fn baseline_v1() -> Result<Self, serde_json::Error> {
        let spec = RlSpec::poche_2p_v1();
        Ok(Self {
            schema_version: 1,
            corpus_id: "poche-baselines-v1".to_owned(),
            spec_id: spec.spec_id.clone(),
            spec_hash: spec.semantic_hash()?,
            reward_id: spec.reward_id,
            seeds: (0_u64..64).map(|index| 0xb453_0000 + index).collect(),
            matchups: vec![
                BaselineMatchup {
                    name: "legal-random-vs-heuristic".to_owned(),
                    seat0: "legal-random".to_owned(),
                    seat1: "heuristic".to_owned(),
                },
                BaselineMatchup {
                    name: "heuristic-vs-legal-random".to_owned(),
                    seat0: "heuristic".to_owned(),
                    seat1: "legal-random".to_owned(),
                },
                BaselineMatchup {
                    name: "legal-random-mirror".to_owned(),
                    seat0: "legal-random".to_owned(),
                    seat1: "legal-random".to_owned(),
                },
                BaselineMatchup {
                    name: "rejection-random-mirror".to_owned(),
                    seat0: "random".to_owned(),
                    seat1: "random".to_owned(),
                },
            ],
            selection_rule: "per matchup select minimum, lower median, maximum score differential for seat0; retain first illegal/failure episode if one exists".to_owned(),
        })
    }

    /// Validate immutable corpus/spec bindings and policy vocabulary.
    ///
    /// # Errors
    ///
    /// Returns a stable manifest error for drift or unsupported policies.
    pub fn validate(&self) -> Result<(), EvaluationError> {
        let expected = Self::baseline_v1().map_err(|_| EvaluationError::Manifest)?;
        if *self != expected {
            return Err(EvaluationError::Manifest);
        }
        Ok(())
    }

    /// Canonical compact manifest hash.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the manifest cannot be encoded.
    pub fn semantic_hash(&self) -> Result<String, serde_json::Error> {
        serde_json::to_vec(self).map(|bytes| blake3::hash(&bytes).to_hex().to_string())
    }
}

/// Aggregate score-first evidence for one matchup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchupSummary {
    pub name: String,
    pub seat_policies: [String; 2],
    pub episodes: usize,
    pub mean_score_seat0: f64,
    pub mean_score_seat1: f64,
    pub mean_score_differential_seat0: f64,
    pub differential_ci95_low: f64,
    pub differential_ci95_high: f64,
    pub illegal_action_count: u64,
    pub mean_game_length_decisions: f64,
    pub best_seed_for_seat0: u64,
    pub median_seed_for_seat0: u64,
    pub worst_seed_for_seat0: u64,
    pub selected_episode_hashes: Vec<String>,
}

/// Complete deterministic baseline evaluation summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineEvaluationSummary {
    pub schema_version: u16,
    pub corpus_id: String,
    pub corpus_hash: String,
    pub spec_id: String,
    pub spec_hash: String,
    pub reward_id: String,
    pub empirical_only: bool,
    pub matchups: Vec<MatchupSummary>,
}

impl BaselineEvaluationSummary {
    /// Canonical summary hash.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the summary cannot be encoded.
    pub fn semantic_hash(&self) -> Result<String, serde_json::Error> {
        serde_json::to_vec(self).map(|bytes| blake3::hash(&bytes).to_hex().to_string())
    }
}

/// Stable baseline evaluation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationError {
    Manifest,
    UnsupportedPolicy,
    Episode(EpisodeError),
    EmptyCorpus,
}

impl From<EpisodeError> for EvaluationError {
    fn from(value: EpisodeError) -> Self {
        Self::Episode(value)
    }
}

/// Evaluate every preregistered matchup and retain only three selected episode
/// hashes per matchup. Full episode serialization remains opt-in.
///
/// # Errors
///
/// Returns a manifest, policy, or episode failure.
pub fn evaluate_baselines(
    manifest: &BaselineCorpusManifest,
) -> Result<BaselineEvaluationSummary, EvaluationError> {
    manifest.validate()?;
    if manifest.seeds.is_empty() {
        return Err(EvaluationError::EmptyCorpus);
    }
    let mut summaries = Vec::with_capacity(manifest.matchups.len());
    for matchup in &manifest.matchups {
        let mut episodes = Vec::with_capacity(manifest.seeds.len());
        for seed in &manifest.seeds {
            let mut seat0 = make_policy(&matchup.seat0, seed ^ 0x5000)?;
            let mut seat1 = make_policy(&matchup.seat1, seed ^ 0x5100)?;
            episodes.push(run_episode_with_seat_policies(
                *seed,
                seat0.as_mut(),
                seat1.as_mut(),
            )?);
        }
        episodes.sort_by_key(|episode| (episode.score_differential_seat0, episode.seed));
        let count_u32 = u32::try_from(episodes.len()).map_err(|_| EvaluationError::EmptyCorpus)?;
        let count = f64::from(count_u32);
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
        let variance = if episodes.len() > 1 {
            episodes
                .iter()
                .map(|episode| {
                    let delta = f64::from(episode.score_differential_seat0) - mean_diff;
                    delta * delta
                })
                .sum::<f64>()
                / f64::from(count_u32 - 1)
        } else {
            0.0
        };
        let margin = 1.96 * (variance / count).sqrt();
        let indices = [0, (episodes.len() - 1) / 2, episodes.len() - 1];
        let selected_episode_hashes = indices
            .into_iter()
            .map(|index| {
                episodes[index]
                    .semantic_hash()
                    .map_err(|_| EvaluationError::Manifest)
            })
            .collect::<Result<Vec<_>, _>>()?;
        summaries.push(MatchupSummary {
            name: matchup.name.clone(),
            seat_policies: [matchup.seat0.clone(), matchup.seat1.clone()],
            episodes: episodes.len(),
            mean_score_seat0: mean0,
            mean_score_seat1: mean1,
            mean_score_differential_seat0: mean_diff,
            differential_ci95_low: mean_diff - margin,
            differential_ci95_high: mean_diff + margin,
            illegal_action_count: episodes
                .iter()
                .map(|episode| episode.illegal_action_count)
                .sum(),
            mean_game_length_decisions: episodes
                .iter()
                .map(|episode| episode.game_length_decisions)
                .map(|value| f64::from(u32::try_from(value).unwrap_or(u32::MAX)))
                .sum::<f64>()
                / count,
            best_seed_for_seat0: episodes[episodes.len() - 1].seed,
            median_seed_for_seat0: episodes[(episodes.len() - 1) / 2].seed,
            worst_seed_for_seat0: episodes[0].seed,
            selected_episode_hashes,
        });
    }
    Ok(BaselineEvaluationSummary {
        schema_version: 1,
        corpus_id: manifest.corpus_id.clone(),
        corpus_hash: manifest
            .semantic_hash()
            .map_err(|_| EvaluationError::Manifest)?,
        spec_id: manifest.spec_id.clone(),
        spec_hash: manifest.spec_hash.clone(),
        reward_id: manifest.reward_id.clone(),
        empirical_only: true,
        matchups: summaries,
    })
}

/// Recreate one exact episode from a checked baseline matchup and corpus seed.
///
/// # Errors
///
/// Returns a manifest, unknown matchup/seed, policy, or episode failure.
pub fn replay_baseline_matchup(
    manifest: &BaselineCorpusManifest,
    matchup_name: &str,
    seed: u64,
) -> Result<crate::EpisodeTranscript, EvaluationError> {
    manifest.validate()?;
    if !manifest.seeds.contains(&seed) {
        return Err(EvaluationError::Manifest);
    }
    let matchup = manifest
        .matchups
        .iter()
        .find(|matchup| matchup.name == matchup_name)
        .ok_or(EvaluationError::Manifest)?;
    let mut seat0 = make_policy(&matchup.seat0, seed ^ 0x5000)?;
    let mut seat1 = make_policy(&matchup.seat1, seed ^ 0x5100)?;
    run_episode_with_seat_policies(seed, seat0.as_mut(), seat1.as_mut())
        .map_err(EvaluationError::Episode)
}

fn make_policy(name: &str, seed: u64) -> Result<Box<dyn Policy>, EvaluationError> {
    match name {
        "random" => Ok(Box::new(UniformRandomPolicy::new(seed))),
        "legal-random" => Ok(Box::new(LegalRandomPolicy::new(seed))),
        "heuristic" => Ok(Box::new(HighCardHeuristicPolicy)),
        _ => Err(EvaluationError::UnsupportedPolicy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_baseline_corpus_is_replayable_score_first_and_seat_swapped() {
        let manifest = BaselineCorpusManifest::baseline_v1().unwrap();
        manifest.validate().unwrap();
        let first = evaluate_baselines(&manifest).unwrap();
        let replay = evaluate_baselines(&manifest).unwrap();
        assert_eq!(first, replay);
        assert_eq!(
            first.semantic_hash().unwrap(),
            replay.semantic_hash().unwrap()
        );
        assert!(first.empirical_only);
        assert_eq!(first.matchups.len(), 4);
        assert!(first.matchups.iter().all(|item| item.episodes == 64));
        assert!(
            first
                .matchups
                .iter()
                .all(|item| item.illegal_action_count == 0)
        );
        assert!(
            first
                .matchups
                .iter()
                .all(|item| item.selected_episode_hashes.len() == 3)
        );
        assert_eq!(
            first.matchups[0].seat_policies,
            ["legal-random", "heuristic"]
        );
        assert_eq!(
            first.matchups[1].seat_policies,
            ["heuristic", "legal-random"]
        );
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// Immutable identifier for the first two-player full-rule RL contract.
pub const SPEC_ID: &str = "poche-2p-v1";
/// Raw round-score reward contract used by [`SPEC_ID`].
pub const REWARD_ID: &str = "round-score-v1";
/// Number of fixed policy actions: bids `0..=7`, then 52 card identities.
pub const ACTION_COUNT: usize = 60;
/// Number of features in the exact v1 observation vector.
pub const OBSERVATION_SIZE: usize = 307;
/// Bid action slots begin here.
pub const BID_OFFSET: usize = 0;
/// Card-identity action slots begin here.
pub const CARD_OFFSET: usize = 8;

/// One named, half-open feature span in the flattened observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSpan {
    pub name: String,
    pub start: usize,
    pub len: usize,
    pub encoding: String,
}

/// User-visible semantic manifest. Any semantic change requires a new ID and
/// consequently a new canonical manifest hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RlSpec {
    pub spec_id: String,
    pub players: usize,
    pub observation_size: usize,
    pub action_count: usize,
    pub feature_spans: Vec<FeatureSpan>,
    pub action_vocabulary: Vec<String>,
    pub history_semantics: String,
    pub chance_semantics: String,
    pub reset_semantics: String,
    pub illegal_action_semantics: String,
    pub reward_id: String,
    pub reward_semantics: String,
    pub terminal_metrics: Vec<String>,
}

impl RlSpec {
    /// Construct the exact checked `poche-2p-v1` manifest.
    #[must_use]
    pub fn poche_2p_v1() -> Self {
        let spans = [
            (
                "phase",
                0,
                5,
                "one_hot(awaiting_deal,bidding,playing,scoring,finished)",
            ),
            ("dealer_relative", 5, 3, "one_hot(none,self,opponent)"),
            (
                "actor_relative",
                8,
                5,
                "one_hot(chance,environment,finished,self,opponent)",
            ),
            ("round_index", 13, 1, "scalar / 13"),
            ("hand_size", 14, 1, "scalar / 7"),
            (
                "private_hand",
                15,
                52,
                "card_identity_bits(clubs_2..spades_ace)",
            ),
            ("hand_counts_relative", 67, 2, "self,opponent each / 7"),
            ("trump", 69, 53, "one_hot(none,clubs_2..spades_ace)"),
            (
                "current_trick",
                122,
                110,
                "two ordered slots: occupied + relative_player[2] + card[52]",
            ),
            ("bids_relative", 232, 18, "self,opponent one_hot(none,0..7)"),
            ("tricks_won_relative", 250, 2, "self,opponent each / 7"),
            (
                "scores_relative",
                252,
                2,
                "self,opponent raw cumulative points / 351",
            ),
            ("pot_cents", 254, 1, "raw cents / 512"),
            (
                "public_played_history",
                255,
                52,
                "card_identity bits for prior/current public plays",
            ),
        ]
        .into_iter()
        .map(|(name, start, len, encoding)| FeatureSpan {
            name: name.to_owned(),
            start,
            len,
            encoding: encoding.to_owned(),
        })
        .collect();
        let mut actions = (0..=7).map(|bid| format!("bid:{bid}")).collect::<Vec<_>>();
        for suit in ["clubs", "diamonds", "hearts", "spades"] {
            for rank in [
                "2", "3", "4", "5", "6", "7", "8", "9", "10", "jack", "queen", "king", "ace",
            ] {
                actions.push(format!("play:{suit}:{rank}"));
            }
        }
        Self {
            spec_id: SPEC_ID.to_owned(),
            players: 2,
            observation_size: OBSERVATION_SIZE,
            action_count: ACTION_COUNT,
            feature_spans: spans,
            action_vocabulary: actions,
            history_semantics: "episode-owned public bitset includes every publicly played card; private opponent/stock identities never enter it".to_owned(),
            chance_semantics: "chance and settlement auto-advance deterministically; policy vocabulary contains neither".to_owned(),
            reset_semantics: "seed selects first dealer and every deal via stable splitmix-derived OracleChanceAction; terminal reset creates a fresh episode".to_owned(),
            illegal_action_semantics: "tests return IllegalAction; production callers must explicitly choose reject-or-count and never remap".to_owned(),
            reward_id: REWARD_ID.to_owned(),
            reward_semantics: "zero except at settlement; each seat receives its raw nonnegative rulebook round points; no clipping or shaping".to_owned(),
            terminal_metrics: vec![
                "raw_cumulative_score".to_owned(),
                "score_differential".to_owned(),
                "round_scores".to_owned(),
                "exact_bid_count".to_owned(),
                "game_length_decisions".to_owned(),
                "illegal_action_count".to_owned(),
                "pot_cents".to_owned(),
                "winner_flags".to_owned(),
            ],
        }
    }

    /// Canonical compact JSON used for human inspection and hashing.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the manifest cannot be encoded.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Lowercase BLAKE3 digest of the canonical manifest.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the manifest cannot be encoded.
    pub fn semantic_hash(&self) -> Result<String, serde_json::Error> {
        self.canonical_json()
            .map(|bytes| blake3::hash(bytes.as_bytes()).to_hex().to_string())
    }

    /// Validate contiguous feature/action shapes and immutable IDs.
    ///
    /// # Errors
    ///
    /// Returns a stable shape/layout category for semantic drift.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.spec_id != SPEC_ID
            || self.reward_id != REWARD_ID
            || self.players != 2
            || self.observation_size != OBSERVATION_SIZE
            || self.action_count != ACTION_COUNT
            || self.action_vocabulary.len() != ACTION_COUNT
        {
            return Err(SpecError::Shape);
        }
        let mut end = 0;
        for span in &self.feature_spans {
            if span.start != end || span.len == 0 {
                return Err(SpecError::FeatureLayout);
            }
            end = end.checked_add(span.len).ok_or(SpecError::FeatureLayout)?;
        }
        if end != OBSERVATION_SIZE {
            return Err(SpecError::FeatureLayout);
        }
        Ok(())
    }
}

/// Manifest validation error without rejected manifest contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecError {
    Shape,
    FeatureLayout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_exact_contiguous_and_hash_bound() {
        let spec = RlSpec::poche_2p_v1();
        assert_eq!(spec.validate(), Ok(()));
        assert_eq!(spec.action_vocabulary[0], "bid:0");
        assert_eq!(spec.action_vocabulary[7], "bid:7");
        assert_eq!(spec.action_vocabulary[8], "play:clubs:2");
        assert_eq!(spec.action_vocabulary[59], "play:spades:ace");
        let hash = spec.semantic_hash().unwrap();
        assert_eq!(
            hash,
            "8852f8568ead1e40aad7bb4ca5b7725340cc01422e077ffddcd5d4f5665bf0bf"
        );
        assert_eq!(
            spec,
            serde_json::from_str(&spec.canonical_json().unwrap()).unwrap()
        );
    }

    #[test]
    fn semantic_changes_force_a_different_hash_and_invalid_id() {
        let spec = RlSpec::poche_2p_v1();
        let base_hash = spec.semantic_hash().unwrap();
        let mut changed = spec.clone();
        changed.reward_semantics.push_str(" changed");
        assert_ne!(base_hash, changed.semantic_hash().unwrap());
        changed.spec_id = "poche-2p-v2".to_owned();
        assert_eq!(changed.validate(), Err(SpecError::Shape));
    }
}

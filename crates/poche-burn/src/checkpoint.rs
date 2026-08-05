// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::{ActorCriticConfig, PpoConfig};

/// Semantic identity required before any checkpoint bytes may be loaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointManifest {
    pub schema_version: u16,
    pub run_id: String,
    pub burn_version: String,
    pub spec_id: String,
    pub spec_hash: String,
    pub reward_id: String,
    pub model: ActorCriticConfig,
    pub ppo: PpoConfig,
    pub seed: u64,
    pub updates: u64,
    pub model_digest: String,
    pub optimizer_digest: String,
    pub policy_probe_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Schema,
    SemanticMismatch,
    Digest,
    Serialization,
}

impl CheckpointManifest {
    /// Validate immutable RL semantics and artifact digests before Burn load.
    ///
    /// # Errors
    /// Returns a stable category without exposing artifact contents.
    pub fn validate(
        &self,
        expected_spec_id: &str,
        expected_spec_hash: &str,
        expected_reward_id: &str,
        model_bytes: &[u8],
        optimizer_bytes: &[u8],
    ) -> Result<(), CheckpointError> {
        if self.schema_version != 1 || self.burn_version != "0.21.0" {
            return Err(CheckpointError::Schema);
        }
        if self.spec_id != expected_spec_id
            || self.spec_hash != expected_spec_hash
            || self.reward_id != expected_reward_id
        {
            return Err(CheckpointError::SemanticMismatch);
        }
        if digest(model_bytes) != self.model_digest
            || digest(optimizer_bytes) != self.optimizer_digest
        {
            return Err(CheckpointError::Digest);
        }
        if self.policy_probe_digest.len() != 64
            || !self
                .policy_probe_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(CheckpointError::Digest);
        }
        Ok(())
    }

    /// Strict canonical JSON for committed run metadata.
    ///
    /// # Errors
    /// Returns a stable serialization category.
    pub fn canonical_json(&self) -> Result<String, CheckpointError> {
        serde_json::to_string(self).map_err(|_| CheckpointError::Serialization)
    }
}

#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use poche_rl::{REWARD_ID, RlSpec, SPEC_ID};

    use super::*;

    fn manifest() -> CheckpointManifest {
        let spec_hash = RlSpec::poche_2p_v1().semantic_hash().unwrap();
        CheckpointManifest {
            schema_version: 1,
            run_id: "checkpoint-test".to_owned(),
            burn_version: "0.21.0".to_owned(),
            spec_id: SPEC_ID.to_owned(),
            spec_hash,
            reward_id: REWARD_ID.to_owned(),
            model: ActorCriticConfig::poche_v1(16),
            ppo: PpoConfig::default(),
            seed: 7,
            updates: 3,
            model_digest: digest(b"model"),
            optimizer_digest: digest(b"optimizer"),
            policy_probe_digest: "a".repeat(64),
        }
    }

    #[test]
    fn checkpoint_rejects_semantic_and_byte_drift_before_loading() {
        let manifest = manifest();
        assert_eq!(
            manifest.validate(
                SPEC_ID,
                &RlSpec::poche_2p_v1().semantic_hash().unwrap(),
                REWARD_ID,
                b"model",
                b"optimizer"
            ),
            Ok(())
        );
        assert_eq!(
            manifest.validate(SPEC_ID, "wrong", REWARD_ID, b"model", b"optimizer"),
            Err(CheckpointError::SemanticMismatch)
        );
        assert_eq!(
            manifest.validate(
                SPEC_ID,
                &RlSpec::poche_2p_v1().semantic_hash().unwrap(),
                REWARD_ID,
                b"changed",
                b"optimizer"
            ),
            Err(CheckpointError::Digest)
        );
    }
}

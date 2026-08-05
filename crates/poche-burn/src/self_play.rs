// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// Immutable historical checkpoint identity; weights live outside Git.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenPolicy {
    pub policy_id: String,
    pub checkpoint_digest: String,
    pub update: u64,
}

/// Bounded append-only pool. Existing digest identities can never be replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenPolicyPool {
    capacity: usize,
    policies: Vec<FrozenPolicy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrozenPoolError {
    EmptyIdentity,
    DuplicateIdentity,
    Full,
}

impl FrozenPolicyPool {
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            policies: Vec::new(),
        }
    }

    /// Append one immutable historical identity.
    ///
    /// # Errors
    /// Rejects empty/duplicate identities and explicit capacity exhaustion.
    pub fn freeze(&mut self, policy: FrozenPolicy) -> Result<(), FrozenPoolError> {
        if policy.policy_id.is_empty() || policy.checkpoint_digest.is_empty() {
            return Err(FrozenPoolError::EmptyIdentity);
        }
        if self
            .policies
            .iter()
            .any(|existing| existing.policy_id == policy.policy_id)
        {
            return Err(FrozenPoolError::DuplicateIdentity);
        }
        if self.policies.len() == self.capacity {
            return Err(FrozenPoolError::Full);
        }
        self.policies.push(policy);
        Ok(())
    }

    #[must_use]
    pub fn policies(&self) -> &[FrozenPolicy] {
        &self.policies
    }

    /// Deterministic seat-randomized matchup selected solely from seed and the
    /// immutable pool. Evaluation opponents are supplied separately.
    #[must_use]
    pub fn rollout_matchup(&self, seed: u64, current_policy: &str) -> [String; 2] {
        let opponent = if self.policies.is_empty() {
            current_policy.to_owned()
        } else {
            let index = usize::try_from(seed % self.policies.len() as u64).unwrap_or(0);
            self.policies[index].policy_id.clone()
        };
        if seed & 1 == 0 {
            [current_policy.to_owned(), opponent]
        } else {
            [opponent, current_policy.to_owned()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_pool_is_bounded_immutable_and_schedule_replays() {
        let mut pool = FrozenPolicyPool::new(2);
        let first = FrozenPolicy {
            policy_id: "historical-0".to_owned(),
            checkpoint_digest: "a".repeat(64),
            update: 0,
        };
        pool.freeze(first.clone()).unwrap();
        assert_eq!(pool.freeze(first), Err(FrozenPoolError::DuplicateIdentity));
        pool.freeze(FrozenPolicy {
            policy_id: "historical-1".to_owned(),
            checkpoint_digest: "b".repeat(64),
            update: 10,
        })
        .unwrap();
        assert_eq!(
            pool.freeze(FrozenPolicy {
                policy_id: "historical-2".to_owned(),
                checkpoint_digest: "c".repeat(64),
                update: 20,
            }),
            Err(FrozenPoolError::Full)
        );
        assert_eq!(
            pool.rollout_matchup(9, "current"),
            pool.rollout_matchup(9, "current")
        );
        assert_eq!(pool.rollout_matchup(9, "current")[1], "current");
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{ACTION_COUNT, ActionIndex, CARD_OFFSET, DecisionView};

/// Framework-independent policy boundary shared by baselines and learners.
pub trait Policy {
    fn name(&self) -> &'static str;
    /// Select one action from the supplied exact viewer decision.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::NoLegalAction`] if the input mask is empty.
    fn select(&mut self, decision: &DecisionView) -> Result<ActionIndex, PolicyError>;
}

/// Policy boundary failure without observation contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    NoLegalAction,
}

/// Deterministic `SplitMix` stream used so replay does not depend on a third-party
/// RNG implementation or version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayRng(u64);

impl ReplayRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn below(&mut self, upper: usize) -> usize {
        let upper = u64::try_from(upper).expect("action vocabulary fits u64");
        usize::try_from(self.next_u64() % upper).expect("bounded value fits usize")
    }
}

/// Uniformly samples the full fixed vocabulary, rejecting masked candidates
/// internally until it reaches a legal action. This is a useful deliberately
/// inefficient random reference and never returns an illegal action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformRandomPolicy {
    rng: ReplayRng,
}

impl UniformRandomPolicy {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            rng: ReplayRng::new(seed),
        }
    }
}

impl Policy for UniformRandomPolicy {
    fn name(&self) -> &'static str {
        "uniform-random-rejection-v1"
    }

    fn select(&mut self, decision: &DecisionView) -> Result<ActionIndex, PolicyError> {
        if !decision.legal_mask.iter().any(|allowed| *allowed) {
            return Err(PolicyError::NoLegalAction);
        }
        loop {
            let index = self.rng.below(ACTION_COUNT);
            if decision.legal_mask[index] {
                return ActionIndex::new(index).map_err(|_| PolicyError::NoLegalAction);
            }
        }
    }
}

/// Uniformly samples the compact list of legal indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegalRandomPolicy {
    rng: ReplayRng,
}

impl LegalRandomPolicy {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            rng: ReplayRng::new(seed),
        }
    }
}

impl Policy for LegalRandomPolicy {
    fn name(&self) -> &'static str {
        "uniform-legal-random-v1"
    }

    fn select(&mut self, decision: &DecisionView) -> Result<ActionIndex, PolicyError> {
        let legal_count = decision
            .legal_mask
            .iter()
            .filter(|allowed| **allowed)
            .count();
        if legal_count == 0 {
            return Err(PolicyError::NoLegalAction);
        }
        let wanted = self.rng.below(legal_count);
        let index = decision
            .legal_mask
            .iter()
            .enumerate()
            .filter(|(_, allowed)| **allowed)
            .nth(wanted)
            .map(|(index, _)| index)
            .ok_or(PolicyError::NoLegalAction)?;
        ActionIndex::new(index).map_err(|_| PolicyError::NoLegalAction)
    }
}

/// Explainable Poche baseline: bid the number of private cards at rank Jack or
/// above, plus low trump cards, clamped by the legal bid mask; when playing,
/// shed the lowest legal card identity. It uses only encoded viewer features.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HighCardHeuristicPolicy;

impl Policy for HighCardHeuristicPolicy {
    fn name(&self) -> &'static str {
        "high-card-low-play-v1"
    }

    fn select(&mut self, decision: &DecisionView) -> Result<ActionIndex, PolicyError> {
        if decision.legal_mask[..CARD_OFFSET]
            .iter()
            .any(|allowed| *allowed)
        {
            let trump = decision.observation[70..122]
                .iter()
                .position(|value| *value > 0.5);
            let mut estimate = 0_usize;
            for card in 0..52 {
                if decision.observation[15 + card] < 0.5 {
                    continue;
                }
                let rank = card % 13;
                if rank >= 9 || trump == Some(card) {
                    estimate += 1;
                }
            }
            return (0..CARD_OFFSET)
                .rev()
                .find(|index| *index <= estimate && decision.legal_mask[*index])
                .or_else(|| {
                    decision.legal_mask[..CARD_OFFSET]
                        .iter()
                        .position(|value| *value)
                })
                .and_then(|index| ActionIndex::new(index).ok())
                .ok_or(PolicyError::NoLegalAction);
        }
        decision.legal_mask[CARD_OFFSET..]
            .iter()
            .position(|allowed| *allowed)
            .and_then(|index| ActionIndex::new(CARD_OFFSET + index).ok())
            .ok_or(PolicyError::NoLegalAction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PocheRlEnv;

    #[test]
    fn all_baselines_are_seed_replayable_and_never_masked() {
        fn actions(mut policy: impl Policy) -> Vec<usize> {
            let mut env = PocheRlEnv::reset(123).unwrap();
            let mut selected = Vec::new();
            while let Some(decision) = env.decision() {
                let action = policy.select(&decision).unwrap();
                assert!(decision.legal_mask[action.get()]);
                selected.push(action.get());
                env.step(action).unwrap();
            }
            selected
        }
        assert_eq!(
            actions(UniformRandomPolicy::new(8)),
            actions(UniformRandomPolicy::new(8))
        );
        assert_eq!(
            actions(LegalRandomPolicy::new(8)),
            actions(LegalRandomPolicy::new(8))
        );
        assert_eq!(
            actions(HighCardHeuristicPolicy),
            actions(HighCardHeuristicPolicy)
        );
    }
}

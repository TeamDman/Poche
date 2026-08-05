// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pure authorization and event-reduction boundary for multiplayer sessions.
//!
//! Concrete room semantics arrive in Task 2.2. This crate already fixes the
//! three-stage API so clocks, transports, renderers, and cryptography cannot be
//! smuggled into deterministic reduction.

use facet::Facet;
use poche_protocol::{CommandEnvelope, DenyReason, PolicyId};

mod machine;
mod projection;
mod state;

pub use machine::{SessionMachine, apply, authorize, decide, decide_transport_disconnect};
pub use projection::*;
pub use state::*;

/// One enforce or audit-only policy result.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct PolicyResult {
    /// Stable policy that produced the result.
    pub policy_id: PolicyId,
    /// Whether this individual policy would allow the attempt.
    pub allowed: bool,
    /// Stable denial reason when `allowed` is false.
    pub reason: Option<DenyReason>,
    /// Audit-only results are recorded but never grant or deny authority.
    pub audit_only: bool,
}

/// Final default-deny authorization decision and its immutable policy evidence.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PolicyDecision {
    /// At least one exact enforce allow applies and no enforce deny applies.
    Allow {
        /// Stable winning allow policy.
        policy_id: PolicyId,
        /// Every applicable enforce and audit-only result.
        results: Vec<PolicyResult>,
    },
    /// Unknown or unauthorized attempts never reach semantic decision.
    Deny {
        /// Stable public denial reason.
        reason: DenyReason,
        /// Winning deny policy when policy evaluation reached that stage.
        policy_id: Option<PolicyId>,
        /// Every applicable enforce and audit-only result.
        results: Vec<PolicyResult>,
    },
}

impl PolicyDecision {
    /// Whether the final enforce result allows semantic decision.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// Command whose exact immutable authorization decision was an allow.
///
/// Construction is private so a reducer cannot accidentally accept a bare
/// decoded command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedCommand {
    command: CommandEnvelope,
    decision: PolicyDecision,
}

impl AuthorizedCommand {
    /// Bind a command to an allow decision.
    ///
    /// # Errors
    ///
    /// Returns the unchanged denial decision when it was not an allow.
    pub fn from_decision(
        command: CommandEnvelope,
        decision: PolicyDecision,
    ) -> Result<Self, PolicyDecision> {
        if decision.is_allowed() {
            Ok(Self { command, decision })
        } else {
            Err(decision)
        }
    }

    /// Borrow the signed command.
    #[must_use]
    pub const fn command(&self) -> &CommandEnvelope {
        &self.command
    }

    /// Borrow the immutable authorization evidence.
    #[must_use]
    pub const fn decision(&self) -> &PolicyDecision {
        &self.decision
    }
}

/// Pure three-stage session contract.
///
/// Implementations may call a policy-neutral game environment from `decide`,
/// but must not perform I/O, read wall time, select chance, or mutate hidden
/// global state.
pub trait PureSessionMachine {
    /// Complete authoritative session state.
    type State: Clone + Eq;
    /// Semantic event produced by an authorized decision.
    type Event: Clone + Eq;
    /// Stable semantic error distinct from authorization denial.
    type Error;

    /// Evaluate structural roles and capabilities without mutation.
    fn authorize(state: &Self::State, command: &CommandEnvelope) -> PolicyDecision;

    /// Decide zero or more events without mutating state.
    ///
    /// # Errors
    ///
    /// Returns a semantic precondition or game error.
    fn decide(
        state: &Self::State,
        command: &AuthorizedCommand,
    ) -> Result<Vec<Self::Event>, Self::Error>;

    /// Apply one already-decided event without I/O.
    ///
    /// # Errors
    ///
    /// Returns an invariant/revision error and leaves the input state unchanged.
    fn apply(state: &Self::State, event: &Self::Event) -> Result<Self::State, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_command_cannot_be_promoted() {
        let decision = PolicyDecision::Deny {
            reason: DenyReason::UnknownCommand,
            policy_id: None,
            results: Vec::new(),
        };
        assert!(!decision.is_allowed());
    }
}

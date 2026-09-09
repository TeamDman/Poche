//! Fresh survivor challenge for creator recovery, not a membership quorum.
use poche_player_client::DeviceClientError;
use poche_protocol::{CorrelationId, PrincipalId};
use std::{collections::BTreeSet, time::Duration};

/// Process-local and deliberately not persisted: restarting issues a new
/// challenge. A previously signed observation cannot satisfy a later restart.
pub struct RecoveryPresence {
    challenge: CorrelationId,
    eligible: BTreeSet<PrincipalId>,
    deadline: Duration,
    witnessed: bool,
}

impl RecoveryPresence {
    /// Eligible principals come from the authenticated checkpoint's remaining
    /// members. The restarting creator never counts as their own survivor.
    pub fn new(creator: &PrincipalId, members: impl IntoIterator<Item = PrincipalId>, now: Duration, grace: Duration) -> Result<Self, DeviceClientError> {
        let deadline = now.checked_add(grace).filter(|_| !grace.is_zero()).ok_or(DeviceClientError::ProtocolViolation)?;
        let mut nonce = [0_u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| DeviceClientError::KeyUnavailable)?;
        let challenge = CorrelationId::new(format!("recovery-{}", data_encoding::HEXLOWER.encode(&nonce))).map_err(|_| DeviceClientError::ProtocolViolation)?;
        Ok(Self { challenge, eligible: members.into_iter().filter(|member| member != creator).collect(), deadline, witnessed: false })
    }

    pub fn challenge(&self) -> &CorrelationId { &self.challenge }

    /// Invoke only AFTER the certified room has verified the request's root,
    /// device signature and current authorization. This method is not a
    /// signature verifier. Correlation must be covered by that signature.
    pub fn accept_authenticated(&mut self, principal: &PrincipalId, correlation: &CorrelationId, now: Duration) -> bool {
        if now >= self.deadline || correlation != &self.challenge || !self.eligible.contains(principal) {
            return false;
        }
        self.witnessed = true;
        true
    }

    pub fn witnessed(&self) -> bool { self.witnessed }

    /// Expiry authorizes no room resurrection. The lifecycle owner must stop
    /// serving and persist disbanding; this helper cannot do that on its own.
    pub fn expired_without_survivor(&self, now: Duration) -> bool {
        !self.witnessed && now >= self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_fresh_authorized_peer_evidence_before_deadline_counts() {
        let creator = PrincipalId::new("creator").unwrap();
        let peer = PrincipalId::new("peer").unwrap();
        let outsider = PrincipalId::new("outsider").unwrap();
        let issue = || RecoveryPresence::new(&creator, [creator.clone(), peer.clone()], Duration::ZERO, Duration::from_secs(60)).unwrap();
        let mut gate = issue();
        let challenge = gate.challenge().clone();
        let old = issue().challenge().clone();
        assert_ne!(challenge, old);
        assert!(!gate.accept_authenticated(&creator, &challenge, Duration::ZERO));
        assert!(!gate.accept_authenticated(&outsider, &challenge, Duration::ZERO));
        assert!(!gate.accept_authenticated(&peer, &old, Duration::ZERO));
        assert!(!gate.witnessed());
        assert!(gate.accept_authenticated(&peer, &challenge, Duration::from_secs(59)));
        assert!(gate.witnessed());
        assert!(!gate.expired_without_survivor(Duration::from_secs(60)));
        let mut expired = issue();
        assert!(!expired.accept_authenticated(&peer, &expired.challenge().clone(), Duration::from_secs(60)));
        assert!(expired.expired_without_survivor(Duration::from_secs(60)));
    }
}

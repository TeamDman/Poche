// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Neutral interchange and agreement checking for independent session tracks.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use facet::Facet;

use crate::{BackendKindWire, ConfidenceKindWire};

/// One Boolean semantic claim normalized independently of backend syntax.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SessionClaimWire {
    /// Stable semantic claim, not a backend-specific assertion name.
    pub claim_id: String,
    /// Truth value asserted by this track.
    pub value: bool,
    /// Stable session-rule origins.
    pub rule_ids: Vec<String>,
}

/// Applicable claim set emitted after one track's native gate succeeds.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SessionTrackEvidenceWire {
    /// Exact shared comparison scope.
    pub scope_id: String,
    /// Independent implementation family.
    pub backend: BackendKindWire,
    /// Strength label; different strengths may agree without becoming equal.
    pub confidence: ConfidenceKindWire,
    /// Backend qualification/omission statement.
    pub qualification: String,
    /// Only claims applicable to this track and scope.
    pub claims: Vec<SessionClaimWire>,
}

/// One cross-track contradiction retained rather than selecting a winner.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SessionDisagreementWire {
    /// Shared semantic claim.
    pub claim_id: String,
    /// Values by backend for classification.
    pub observations: Vec<SessionClaimObservationWire>,
}

/// Backend/value pair within a disagreement.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SessionClaimObservationWire {
    /// Producing backend.
    pub backend: BackendKindWire,
    /// Reported truth value.
    pub value: bool,
}

/// Complete neutral comparison result.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SessionAgreementWire {
    /// Exact common scope.
    pub scope_id: String,
    /// Number of independent track records.
    pub tracks: usize,
    /// Claims shared by at least two tracks.
    pub compared_claims: usize,
    /// Total backend observations participating in those comparisons.
    pub observations: usize,
    /// Empty on agreement; contradictions never resolve by backend priority.
    pub disagreements: Vec<SessionDisagreementWire>,
}

/// Malformed evidence that cannot safely enter comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionAgreementError(String);

impl fmt::Display for SessionAgreementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SessionAgreementError {}

/// Compare every claim shared by two or more applicable independent tracks.
///
/// No backend is privileged. A differing value is returned as a disagreement;
/// malformed scope/backend/claim identity is an error.
///
/// # Errors
///
/// Returns an error for fewer than two tracks, mixed/empty scopes, duplicate
/// backends, empty qualifications/rule sets, or duplicate claims within a
/// track.
pub fn compare_session_tracks(
    evidence: &[SessionTrackEvidenceWire],
) -> Result<SessionAgreementWire, SessionAgreementError> {
    let Some(first) = evidence.first() else {
        return Err(problem("session comparison requires at least two tracks"));
    };
    if evidence.len() < 2 || first.scope_id.is_empty() {
        return Err(problem(
            "session comparison requires at least two tracks and a nonempty scope",
        ));
    }

    let mut backends = BTreeSet::new();
    let mut grouped = BTreeMap::<String, Vec<SessionClaimObservationWire>>::new();
    for track in evidence {
        if track.scope_id != first.scope_id || track.qualification.is_empty() {
            return Err(problem("session track scope or qualification is invalid"));
        }
        let backend_key = format!("{:?}", track.backend);
        if !backends.insert(backend_key) {
            return Err(problem("duplicate session backend evidence"));
        }
        let mut claims = BTreeSet::new();
        for claim in &track.claims {
            if claim.claim_id.is_empty()
                || claim.rule_ids.is_empty()
                || !claims.insert(claim.claim_id.as_str())
            {
                return Err(problem("invalid or duplicate session claim"));
            }
            grouped
                .entry(claim.claim_id.clone())
                .or_default()
                .push(SessionClaimObservationWire {
                    backend: track.backend,
                    value: claim.value,
                });
        }
    }

    let mut compared_claims = 0;
    let mut observations = 0;
    let mut disagreements = Vec::new();
    for (claim_id, values) in grouped {
        if values.len() < 2 {
            continue;
        }
        compared_claims += 1;
        observations += values.len();
        if values.iter().any(|item| item.value != values[0].value) {
            disagreements.push(SessionDisagreementWire {
                claim_id,
                observations: values,
            });
        }
    }
    if compared_claims == 0 {
        return Err(problem("session tracks share no comparable claims"));
    }
    Ok(SessionAgreementWire {
        scope_id: first.scope_id.clone(),
        tracks: evidence.len(),
        compared_claims,
        observations,
        disagreements,
    })
}

fn problem(message: impl Into<String>) -> SessionAgreementError {
    SessionAgreementError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(backend: BackendKindWire, value: bool) -> SessionTrackEvidenceWire {
        SessionTrackEvidenceWire {
            scope_id: "lobby-micro".to_owned(),
            backend,
            confidence: ConfidenceKindWire::Bounded,
            qualification: "test bound".to_owned(),
            claims: vec![SessionClaimWire {
                claim_id: "default-deny".to_owned(),
                value,
                rule_ids: vec!["S-AUTH-001".to_owned()],
            }],
        }
    }

    #[test]
    fn agreement_does_not_privilege_rust() {
        let report = compare_session_tracks(&[
            track(BackendKindWire::Alloy, true),
            track(BackendKindWire::ScryerProlog, false),
        ])
        .unwrap();
        assert_eq!(report.compared_claims, 1);
        assert_eq!(report.disagreements.len(), 1);
    }

    #[test]
    fn duplicate_backend_fails_closed() {
        let evidence = [
            track(BackendKindWire::RustExplicit, true),
            track(BackendKindWire::RustExplicit, true),
        ];
        assert!(compare_session_tracks(&evidence).is_err());
    }
}

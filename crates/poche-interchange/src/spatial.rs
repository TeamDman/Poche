// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Neutral interchange and agreement checking for independent spatial tracks.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use facet::Facet;

use crate::{BackendKindWire, ConfidenceKindWire};

/// Complete stable spatial/refinement obligation catalog for phase three.
pub const SPATIAL_OBLIGATION_IDS: [&str; 15] = [
    "P3-S-IDENTITY",
    "P3-S-CONSERVATION",
    "P3-S-ZONE-EXCLUSIVITY",
    "P3-S-AMBIGUITY",
    "P3-S-ATTACHMENT",
    "P3-S-PRIVACY",
    "P3-S-ROUNDTRIP",
    "P3-S-PLAY-EQUIVALENCE",
    "P3-S-ONE-CARD-MOVE",
    "P3-S-PAUSE-IMMOBILE",
    "P3-S-RECOVERY-IMMOBILE",
    "P3-S-TRANSIT-PRESENTATION",
    "P3-S-FAIR-CONVERGENCE",
    "P3-S-PREDECESSOR",
    "P3-S-CONTINUOUS-RENDERER",
];

/// One stable Boolean spatial/refinement claim.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SpatialClaimWire {
    /// Cross-track semantic identity, independent of backend assertion names.
    pub claim_id: String,
    /// Truth value reported by this independently executed track.
    pub value: bool,
    /// Stable refinement obligations supporting the claim.
    pub obligation_ids: Vec<String>,
}

/// Applicable claims emitted only after one source gate succeeds.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SpatialTrackEvidenceWire {
    /// Common projected comparison vocabulary/scope.
    pub comparison_scope_id: String,
    /// Exact native scope retained without widening.
    pub native_scope_id: String,
    /// Independent implementation family.
    pub backend: BackendKindWire,
    /// Track-specific evidence strength.
    pub confidence: ConfidenceKindWire,
    /// Human-readable native qualification.
    pub qualification: String,
    /// Explicitly excluded or abstracted behavior.
    pub exclusions: Vec<String>,
    /// Only claims actually supported by this track.
    pub claims: Vec<SpatialClaimWire>,
}

/// Backend/value pair within a cross-track disagreement.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SpatialClaimObservationWire {
    /// Producing independent backend.
    pub backend: BackendKindWire,
    /// Reported value.
    pub value: bool,
}

/// Contradiction retained without assigning any backend priority.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SpatialDisagreementWire {
    /// Shared semantic claim.
    pub claim_id: String,
    /// Values from every applicable track.
    pub observations: Vec<SpatialClaimObservationWire>,
}

/// Complete neutral spatial comparison.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SpatialAgreementWire {
    /// Common projected scope.
    pub comparison_scope_id: String,
    /// Number of independent tracks.
    pub tracks: usize,
    /// Claims observed by at least two tracks.
    pub compared_claims: usize,
    /// Backend observations participating in comparisons.
    pub observations: usize,
    /// Claims intentionally supported by only one track.
    pub unshared_claims: Vec<String>,
    /// Contradictions, with no preferred source.
    pub disagreements: Vec<SpatialDisagreementWire>,
}

/// Malformed spatial evidence that cannot safely enter comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialAgreementError(String);

impl fmt::Display for SpatialAgreementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialAgreementError {}

/// Compare every spatial claim shared by two or more applicable tracks.
///
/// Native scope IDs, strengths, qualifications, and exclusions are retained;
/// they do not have to be equal. No backend is privileged.
///
/// # Errors
///
/// Rejects fewer than two tracks, mixed comparison scopes, duplicate
/// backends/claims, empty native qualifications/exclusions, or claims without
/// stable obligation IDs.
pub fn compare_spatial_tracks(
    evidence: &[SpatialTrackEvidenceWire],
) -> Result<SpatialAgreementWire, SpatialAgreementError> {
    let Some(first) = evidence.first() else {
        return Err(problem("spatial comparison requires at least two tracks"));
    };
    if evidence.len() < 2 || first.comparison_scope_id.is_empty() {
        return Err(problem(
            "spatial comparison requires two tracks and a projected scope",
        ));
    }

    let mut backends = BTreeSet::new();
    let mut grouped = BTreeMap::<String, Vec<SpatialClaimObservationWire>>::new();
    for track in evidence {
        if track.comparison_scope_id != first.comparison_scope_id
            || track.native_scope_id.is_empty()
            || track.qualification.is_empty()
            || track.exclusions.is_empty()
            || track.exclusions.iter().any(String::is_empty)
        {
            return Err(problem("spatial track scope/qualification is incomplete"));
        }
        let backend_key = format!("{:?}", track.backend);
        if !backends.insert(backend_key) {
            return Err(problem("duplicate spatial backend evidence"));
        }
        let mut claims = BTreeSet::new();
        for claim in &track.claims {
            if claim.claim_id.is_empty()
                || claim.obligation_ids.is_empty()
                || claim.obligation_ids.iter().any(String::is_empty)
                || !claims.insert(claim.claim_id.as_str())
            {
                return Err(problem("invalid or duplicate spatial claim"));
            }
            grouped
                .entry(claim.claim_id.clone())
                .or_default()
                .push(SpatialClaimObservationWire {
                    backend: track.backend,
                    value: claim.value,
                });
        }
    }

    let mut compared_claims = 0;
    let mut observations = 0;
    let mut unshared_claims = Vec::new();
    let mut disagreements = Vec::new();
    for (claim_id, values) in grouped {
        if values.len() == 1 {
            unshared_claims.push(claim_id);
            continue;
        }
        compared_claims += 1;
        observations += values.len();
        if values.iter().any(|item| item.value != values[0].value) {
            disagreements.push(SpatialDisagreementWire {
                claim_id,
                observations: values,
            });
        }
    }
    if compared_claims == 0 {
        return Err(problem("spatial tracks share no comparable claims"));
    }
    Ok(SpatialAgreementWire {
        comparison_scope_id: first.comparison_scope_id.clone(),
        tracks: evidence.len(),
        compared_claims,
        observations,
        unshared_claims,
        disagreements,
    })
}

fn problem(message: impl Into<String>) -> SpatialAgreementError {
    SpatialAgreementError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        backend: BackendKindWire,
        native_scope: &str,
        value: bool,
    ) -> SpatialTrackEvidenceWire {
        SpatialTrackEvidenceWire {
            comparison_scope_id: "spatial-micro".to_owned(),
            native_scope_id: native_scope.to_owned(),
            backend,
            confidence: ConfidenceKindWire::Bounded,
            qualification: "test qualification".to_owned(),
            exclusions: vec!["continuous meshes".to_owned()],
            claims: vec![SpatialClaimWire {
                claim_id: "P3-S-PRIVACY".to_owned(),
                value,
                obligation_ids: vec!["P3-S-PRIVACY".to_owned()],
            }],
        }
    }

    #[test]
    fn spatial_agreement_preserves_native_scopes_and_does_not_privilege_rust() {
        let report = compare_spatial_tracks(&[
            track(BackendKindWire::Alloy, "alloy-bound", true),
            track(BackendKindWire::RustExplicit, "rust-samples", false),
        ])
        .unwrap();
        assert_eq!(report.compared_claims, 1);
        assert_eq!(report.disagreements.len(), 1);
    }

    #[test]
    fn duplicate_backend_or_missing_exclusion_fails_closed() {
        let mut first = track(BackendKindWire::NuSmv, "symbolic", true);
        let second = first.clone();
        assert!(compare_spatial_tracks(&[first.clone(), second]).is_err());
        first.exclusions.clear();
        assert!(
            compare_spatial_tracks(
                &[first, track(BackendKindWire::ScryerProlog, "queries", true),]
            )
            .is_err()
        );
    }
}

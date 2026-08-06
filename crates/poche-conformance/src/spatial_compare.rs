// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Neutral comparison of independently gated spatial evidence tracks.

use std::{error::Error, fmt, path::Path};

use poche_check::check_spatial_refinement;
use poche_interchange::{
    BackendKindWire, ConfidenceKindWire, SpatialAgreementWire, SpatialClaimWire,
    SpatialTrackEvidenceWire, compare_spatial_tracks,
};

use crate::{
    check_spatial_alloy_layout_micro, check_spatial_nusmv_transition_micro,
    check_spatial_prolog_query_micro,
};

const COMPARISON_SCOPE: &str = "spatial-micro";

/// Successful four-track spatial comparison without an oracle hierarchy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialComparisonReport {
    /// Neutral agreement/disagreement result.
    pub agreement: SpatialAgreementWire,
    /// Exact evidence records, including native scopes and exclusions.
    pub tracks: Vec<SpatialTrackEvidenceWire>,
    /// Number of independent source gates executed successfully.
    pub source_gates: usize,
}

/// Native gate or neutral-evidence validation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialComparisonError(String);

impl fmt::Display for SpatialComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialComparisonError {}

/// Execute Rust, Alloy, `NuSMV`, and Scryer gates, then compare only their
/// genuinely shared Boolean abstractions.
///
/// # Errors
///
/// Returns the first source-gate or malformed-neutral-evidence failure.
/// Semantic disagreements are retained in the successful report.
pub fn compare_spatial_models(
    root: &Path,
) -> Result<SpatialComparisonReport, SpatialComparisonError> {
    let rust = check_spatial_refinement()
        .map_err(|error| problem(format!("Rust spatial gate failed: {error}")))?;
    let alloy = check_spatial_alloy_layout_micro(root)
        .map_err(|error| problem(format!("Alloy spatial gate failed: {error}")))?;
    let nusmv = check_spatial_nusmv_transition_micro(root)
        .map_err(|error| problem(format!("NuSMV spatial gate failed: {error}")))?;
    let prolog = check_spatial_prolog_query_micro(root)
        .map_err(|error| problem(format!("Prolog spatial gate failed: {error}")))?;

    let tracks = spatial_track_evidence(rust.scope, alloy.scope, nusmv.scope, prolog.scope);
    let agreement = compare_spatial_tracks(&tracks)
        .map_err(|error| problem(format!("neutral spatial evidence is invalid: {error}")))?;
    Ok(SpatialComparisonReport {
        agreement,
        tracks,
        source_gates: 4,
    })
}

pub(crate) fn spatial_track_evidence(
    rust_scope: &str,
    alloy_scope: &str,
    nusmv_scope: &str,
    prolog_scope: &str,
) -> Vec<SpatialTrackEvidenceWire> {
    use BackendKindWire::{Alloy, NuSmv, RustExplicit, ScryerProlog};
    use ConfidenceKindWire::{Bounded, Queried, Sampled, Symbolic};

    let claims = |ids: &[&str]| {
        ids.iter()
            .map(|id| SpatialClaimWire {
                claim_id: (*id).to_owned(),
                value: true,
                obligation_ids: vec![(*id).to_owned()],
            })
            .collect()
    };
    vec![
        SpatialTrackEvidenceWire {
            comparison_scope_id: COMPARISON_SCOPE.to_owned(),
            native_scope_id: rust_scope.to_owned(),
            backend: RustExplicit,
            confidence: Sampled,
            qualification: "all 2-8 registered layouts plus 16 complete deterministic two-player traces and four injected faults".to_owned(),
            exclusions: vec![
                "not exhaustive over all deck orders or player policies".to_owned(),
                "renderer pixels, continuous meshes, physics, network, and cryptography excluded".to_owned(),
            ],
            claims: claims(&[
                "P3-S-IDENTITY",
                "P3-S-CONSERVATION",
                "P3-S-ZONE-EXCLUSIVITY",
                "P3-S-AMBIGUITY",
                "P3-S-ATTACHMENT",
                "P3-S-PRIVACY",
                "P3-S-ROUNDTRIP",
                "P3-S-PLAY-EQUIVALENCE",
                "P3-S-ONE-CARD-MOVE",
                "P3-S-TRANSIT-PRESENTATION",
            ]),
        },
        SpatialTrackEvidenceWire {
            comparison_scope_id: COMPARISON_SCOPE.to_owned(),
            native_scope_id: alloy_scope.to_owned(),
            backend: Alloy,
            confidence: Bounded,
            qualification: "two players, three viewers, eight cards/slots, seven discrete coordinate cells, 5-bit integers".to_owned(),
            exclusions: vec![
                "52-card universe and layouts above two players excluded".to_owned(),
                "continuous coordinates, temporal transitions, renderer output, and physics excluded".to_owned(),
            ],
            claims: claims(&[
                "P3-S-IDENTITY",
                "P3-S-ZONE-EXCLUSIVITY",
                "P3-S-AMBIGUITY",
                "P3-S-ATTACHMENT",
                "P3-S-PRIVACY",
                "P3-S-ROUNDTRIP",
            ]),
        },
        SpatialTrackEvidenceWire {
            comparison_scope_id: COMPARISON_SCOPE.to_owned(),
            native_scope_id: nusmv_scope.to_owned(),
            backend: NuSmv,
            confidence: Symbolic,
            qualification: "two stable cards/viewers with finite endpoints, transit, pause/recovery, and explicit scheduler modes".to_owned(),
            exclusions: vec![
                "card faces, 52-card conservation, coordinates, slots, and text glyphs excluded".to_owned(),
                "fair convergence is conditional on the named fair_animation mode".to_owned(),
            ],
            claims: claims(&[
                "P3-S-IDENTITY",
                "P3-S-PRIVACY",
                "P3-S-ONE-CARD-MOVE",
                "P3-S-PAUSE-IMMOBILE",
                "P3-S-RECOVERY-IMMOBILE",
                "P3-S-TRANSIT-PRESENTATION",
                "P3-S-FAIR-CONVERGENCE",
            ]),
        },
        SpatialTrackEvidenceWire {
            comparison_scope_id: COMPARISON_SCOPE.to_owned(),
            native_scope_id: prolog_scope.to_owned(),
            backend: ScryerProlog,
            confidence: Queried,
            qualification: "63-row finite ground corpus over eight cards/drops, four layout findings, and seven reversible transitions".to_owned(),
            exclusions: vec![
                "arbitrary real/nonlinear constraints, continuous motion, and mesh/collision solving excluded".to_owned(),
                "pause/recovery endpoint preservation and one-card mutation are explanatory facts, not structural theorems".to_owned(),
            ],
            claims: claims(&[
                "P3-S-IDENTITY",
                "P3-S-AMBIGUITY",
                "P3-S-ATTACHMENT",
                "P3-S-PRIVACY",
                "P3-S-PLAY-EQUIVALENCE",
                "P3-S-PREDECESSOR",
            ]),
        },
    ]
}

fn problem(message: impl Into<String>) -> SpatialComparisonError {
    SpatialComparisonError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_spatial_sources_run_before_neutral_comparison() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = compare_spatial_models(&root).expect("spatial sources should agree");
        assert_eq!(report.source_gates, 4);
        assert_eq!(report.agreement.tracks, 4);
        assert_eq!(report.agreement.compared_claims, 9);
        assert_eq!(report.agreement.observations, 24);
        assert_eq!(report.agreement.unshared_claims.len(), 5);
        assert!(report.agreement.disagreements.is_empty());
        assert!(
            report
                .tracks
                .iter()
                .all(|track| !track.exclusions.is_empty())
        );
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Independent bounded Alloy evidence for the spatial refinement contract.

use std::{error::Error, fmt, path::Path};

use poche_native_tools::{
    AlloyCommandExpectation, AlloyCommandKind, AlloyOutcome, NativeDisposition, run_alloy_suite,
};

const SUITE_ID: &str = "spatial-alloy-layout-micro";
const MODEL_PATH: &str = "models/alloy/spatial.als";
const SCOPE_ID: &str = "layout-micro-2p-3v-8c-7z-7cells-8slots-int5";

const POSITIVE_ASSERTIONS: [&str; 7] = [
    "SeatAndOwnedZoneInjection",
    "ZoneSeparation",
    "CardLocationAndSlotUniqueness",
    "FaceAttachmentTotality",
    "VisibilityRelations",
    "ScoreAttachmentTotality",
    "RealizationAbstractionRoundTrip",
];

const NEGATIVE_CONTROLS: [&str; 3] = [
    "OverlapNegativeControl",
    "AmbiguousFirstMatchNegativeControl",
    "DetachedFaceTextNegativeControl",
];

/// Successful normalized bounded Alloy receipt for `layout-micro`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialAlloyReport {
    /// Stable declared bounded scope.
    pub scope: &'static str,
    /// Total native commands with exact receipt-derived source.
    pub command_count: usize,
    /// Satisfiable canonical spatial instances.
    pub canonical_witnesses: usize,
    /// Correct spatial assertions proved UNSAT within the bound.
    pub positive_assertions: usize,
    /// Deliberately false assertions with retained SAT counterexamples.
    pub negative_controls: usize,
    /// Exact source of every command, including bitwidth and atom bounds.
    pub command_scopes: Vec<String>,
    /// Ignored evidence directory containing raw output, normalized results,
    /// and Alloy's counterexample-bearing `receipt.json`.
    pub evidence_directory: String,
}

/// Failure to obtain complete, discriminating Alloy spatial evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialAlloyError(String);

impl fmt::Display for SpatialAlloyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialAlloyError {}

/// Run the handwritten Alloy spatial model and require every positive and
/// negative-control polarity in the named finite scope.
///
/// # Errors
///
/// Fails closed when Alloy is unavailable, output is unrecognized, a command
/// polarity differs, a scope is missing, or a SAT control has no instance.
pub fn check_spatial_alloy_layout_micro(
    root: &Path,
) -> Result<SpatialAlloyReport, SpatialAlloyError> {
    use AlloyCommandKind::{Assertion, Witness};
    use AlloyOutcome::{Sat, Unsat};

    let mut expectations = vec![AlloyCommandExpectation {
        name: "CanonicalSpatialWitness".to_owned(),
        kind: Witness,
        outcome: Sat,
    }];
    expectations.extend(POSITIVE_ASSERTIONS.map(|name| AlloyCommandExpectation {
        name: name.to_owned(),
        kind: Assertion,
        outcome: Unsat,
    }));
    expectations.extend(NEGATIVE_CONTROLS.map(|name| AlloyCommandExpectation {
        name: name.to_owned(),
        kind: Assertion,
        outcome: Sat,
    }));

    let native = run_alloy_suite(root, SUITE_ID, Path::new(MODEL_PATH), &expectations);
    if native.disposition != NativeDisposition::Success {
        return Err(problem(format!(
            "native Alloy suite was {:?}: {} ({})",
            native.disposition,
            native.diagnostic,
            native.evidence_directory.display()
        )));
    }
    if native.results.len() != expectations.len() {
        return Err(problem(format!(
            "expected {} Alloy results, received {}",
            expectations.len(),
            native.results.len()
        )));
    }
    let command_scopes = native
        .results
        .iter()
        .map(|result| {
            result
                .command_source
                .clone()
                .ok_or_else(|| problem(format!("{} omitted its command scope", result.name)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if command_scopes
        .iter()
        .any(|source| !source.contains("5 Int"))
    {
        return Err(problem(
            "one or more Alloy commands omitted the 5-bit integer scope",
        ));
    }
    for name in std::iter::once("CanonicalSpatialWitness").chain(NEGATIVE_CONTROLS) {
        let result = native
            .results
            .iter()
            .find(|result| result.name == name)
            .ok_or_else(|| problem(format!("missing SAT evidence for {name}")))?;
        if result.instances != Some((1, 1)) {
            return Err(problem(format!(
                "{name} did not retain exactly one requested SAT instance: {:?}",
                result.instances
            )));
        }
    }

    Ok(SpatialAlloyReport {
        scope: SCOPE_ID,
        command_count: native.results.len(),
        canonical_witnesses: 1,
        positive_assertions: POSITIVE_ASSERTIONS.len(),
        negative_controls: NEGATIVE_CONTROLS.len(),
        command_scopes,
        evidence_directory: native.evidence_directory.display().to_string(),
    })
}

fn problem(message: impl Into<String>) -> SpatialAlloyError {
    SpatialAlloyError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_alloy_layout_micro_is_bounded_and_discriminating() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = check_spatial_alloy_layout_micro(&root)
            .expect("spatial Alloy model should satisfy its declared polarities");
        assert_eq!(report.command_count, 11);
        assert_eq!(report.canonical_witnesses, 1);
        assert_eq!(report.positive_assertions, 7);
        assert_eq!(report.negative_controls, 3);
        assert_eq!(report.command_scopes.len(), 11);
    }
}

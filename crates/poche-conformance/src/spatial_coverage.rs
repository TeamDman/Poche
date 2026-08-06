// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Machine audit for the spatial obligation/track coverage ledger.

use std::{collections::BTreeMap, error::Error, fmt, fs, path::Path};

use poche_interchange::{BackendKindWire, SPATIAL_OBLIGATION_IDS};

use crate::spatial_compare::spatial_track_evidence;

const LEDGER_PATH: &str = "docs/spatial-coverage.md";

/// Successful classification of every spatial obligation against every track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialCoverageReport {
    /// Stable obligations present exactly once.
    pub obligations: usize,
    /// Independently authored implementation families.
    pub tracks: usize,
    /// All applicable and reasoned-not-applicable cells.
    pub classified_cells: usize,
    /// Cells backed by a claim from that native track.
    pub applicable_cells: usize,
}

/// Missing, duplicate, malformed, or evidence-inconsistent ledger content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialCoverageError(String);

impl fmt::Display for SpatialCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialCoverageError {}

/// Audit the checked coverage ledger against the stable obligation catalog and
/// the exact claims registered for the four native spatial evidence tracks.
///
/// # Errors
///
/// Rejects missing/duplicate obligations, malformed or unclassified cells,
/// and any disagreement between an applicable ledger cell and track evidence.
pub fn audit_spatial_coverage(root: &Path) -> Result<SpatialCoverageReport, SpatialCoverageError> {
    let path = root.join(LEDGER_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|error| problem(format!("could not read {}: {error}", path.display())))?;
    let rows = parse_rows(&text)?;
    let expected = SPATIAL_OBLIGATION_IDS
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let actual = rows.keys().cloned().collect::<Vec<_>>();
    let mut sorted_expected = expected.clone();
    sorted_expected.sort();
    if actual != sorted_expected {
        return Err(problem(format!(
            "spatial obligation inventory mismatch: expected={sorted_expected:?} actual={actual:?}"
        )));
    }

    let tracks = spatial_track_evidence("rust", "alloy", "nusmv", "prolog");
    let columns = [
        (BackendKindWire::RustExplicit, "sampled:"),
        (BackendKindWire::Alloy, "bounded:"),
        (BackendKindWire::NuSmv, "symbolic:"),
        (BackendKindWire::ScryerProlog, "queried:"),
    ];
    let mut applicable_cells = 0;
    for obligation in expected {
        let cells = &rows[&obligation];
        for (column, (backend, prefix)) in columns.iter().enumerate() {
            let cell = &cells[column];
            let applicable = cell.starts_with(prefix);
            if !applicable && !cell.starts_with("N/A:") {
                return Err(problem(format!(
                    "{obligation} {backend:?} cell needs `{prefix}` or `N/A:`: {cell}"
                )));
            }
            let track = tracks
                .iter()
                .find(|track| track.backend == *backend)
                .ok_or_else(|| problem(format!("missing {backend:?} evidence track")))?;
            let registered = track
                .claims
                .iter()
                .any(|claim| claim.claim_id == obligation);
            if applicable != registered {
                return Err(problem(format!(
                    "{obligation} {backend:?} applicability disagrees with registered track evidence"
                )));
            }
            applicable_cells += usize::from(applicable);
        }
    }

    Ok(SpatialCoverageReport {
        obligations: rows.len(),
        tracks: columns.len(),
        classified_cells: rows.len() * columns.len(),
        applicable_cells,
    })
}

fn parse_rows(text: &str) -> Result<BTreeMap<String, [String; 4]>, SpatialCoverageError> {
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let cells = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        let Some(id) = cells
            .first()
            .map(|cell| cell.trim_matches('`'))
            .filter(|cell| cell.starts_with("P3-S-"))
        else {
            continue;
        };
        if cells.len() != 6 {
            return Err(problem(format!(
                "{id} coverage row has {} rather than 6 columns",
                cells.len()
            )));
        }
        if cells[1].is_empty() || cells[2..].iter().any(|cell| cell.is_empty()) {
            return Err(problem(format!("{id} coverage row has an empty cell")));
        }
        let track_cells = [
            cells[2].to_owned(),
            cells[3].to_owned(),
            cells[4].to_owned(),
            cells[5].to_owned(),
        ];
        if rows.insert(id.to_owned(), track_cells).is_some() {
            return Err(problem(format!("duplicate spatial coverage row {id}")));
        }
    }
    Ok(rows)
}

fn problem(message: impl Into<String>) -> SpatialCoverageError {
    SpatialCoverageError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_ledger_classifies_every_track_without_scope_inference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = audit_spatial_coverage(&root).expect("coverage ledger should be exact");
        assert_eq!(report.obligations, 15);
        assert_eq!(report.tracks, 4);
        assert_eq!(report.classified_cells, 60);
        assert_eq!(report.applicable_cells, 29);
    }
}

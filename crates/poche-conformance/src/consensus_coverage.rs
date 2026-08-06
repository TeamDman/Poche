// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Machine audit for the replicated-consensus coverage ledger.

use std::{collections::BTreeMap, error::Error, fmt, fs, path::Path};

use poche_interchange::BackendKindWire;

use crate::consensus_compare::{CONSENSUS_OBLIGATION_IDS, consensus_track_evidence};

const LEDGER_PATH: &str = "docs/consensus-coverage.md";

/// Successful classification of each consensus obligation and evidence track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusCoverageReport {
    pub obligations: usize,
    pub tracks: usize,
    pub classified_cells: usize,
    pub applicable_cells: usize,
}

/// Missing, duplicate, malformed, or evidence-inconsistent coverage content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusCoverageError(String);

impl fmt::Display for ConsensusCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConsensusCoverageError {}

/// Audit every ledger cell against the exact claims emitted by all four
/// independently qualified consensus tracks.
///
/// # Errors
///
/// Rejects missing/duplicate obligations, malformed/unclassified cells, and
/// applicable cells that disagree with registered native evidence.
pub fn audit_consensus_coverage(
    root: &Path,
) -> Result<ConsensusCoverageReport, ConsensusCoverageError> {
    let path = root.join(LEDGER_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|error| problem(format!("could not read {}: {error}", path.display())))?;
    let rows = parse_rows(&text)?;
    let expected = CONSENSUS_OBLIGATION_IDS.map(str::to_owned);
    let mut sorted_expected = expected.clone();
    sorted_expected.sort();
    if rows.keys().cloned().collect::<Vec<_>>() != sorted_expected {
        return Err(problem(format!(
            "consensus obligation inventory mismatch: expected={sorted_expected:?} actual={:?}",
            rows.keys().collect::<Vec<_>>()
        )));
    }

    let tracks = consensus_track_evidence();
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
                    "{obligation} {backend:?} needs `{prefix}` or `N/A:`: {cell}"
                )));
            }
            let track = tracks
                .iter()
                .find(|track| track.backend == *backend)
                .ok_or_else(|| problem(format!("missing {backend:?} consensus track")))?;
            let registered = track
                .claims
                .iter()
                .any(|claim| claim.claim_id == obligation);
            if applicable != registered {
                return Err(problem(format!(
                    "{obligation} {backend:?} applicability disagrees with registered evidence"
                )));
            }
            applicable_cells += usize::from(applicable);
        }
    }

    Ok(ConsensusCoverageReport {
        obligations: rows.len(),
        tracks: columns.len(),
        classified_cells: rows.len() * columns.len(),
        applicable_cells,
    })
}

fn parse_rows(text: &str) -> Result<BTreeMap<String, [String; 4]>, ConsensusCoverageError> {
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
            .filter(|cell| cell.starts_with("P3-C-"))
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
            return Err(problem(format!("duplicate consensus coverage row {id}")));
        }
    }
    Ok(rows)
}

fn problem(message: impl Into<String>) -> ConsensusCoverageError {
    ConsensusCoverageError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_ledger_classifies_every_consensus_track() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = audit_consensus_coverage(&root).expect("coverage ledger should be exact");
        assert_eq!(report.obligations, 10);
        assert_eq!(report.tracks, 4);
        assert_eq!(report.classified_cells, 40);
        assert_eq!(report.applicable_cells, 36);
    }
}

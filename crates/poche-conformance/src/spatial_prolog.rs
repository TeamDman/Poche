// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Normalized Scryer Prolog evidence for finite spatial queries.

use std::{collections::BTreeSet, error::Error, fmt, path::Path};

use poche_native_tools::{NativeDisposition, run_prolog_model_fixture};

const MODEL_PATH: &str = "models/prolog/spatial.pl";
const SCOPE_ID: &str = "query-micro-2p-3v-8cards-8drops-4layouts-7transitions";

const FIXTURES: [(&str, usize); 7] = [
    ("card_locations", 8),
    ("attached_text", 24),
    ("command_resolution", 5),
    ("drop_explanations", 8),
    ("layout_findings", 4),
    ("successors", 7),
    ("predecessors", 7),
];

/// One exact normalized Scryer answer set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialPrologFixture {
    /// Ground native fixture goal.
    pub goal: &'static str,
    /// Duplicate-free normalized answer count.
    pub answer_count: usize,
    /// Length-framed digest of answers in lexical order.
    pub digest: String,
    /// Ignored raw/normalized evidence directory.
    pub evidence_directory: String,
}

/// Successful receipt for the complete finite spatial query corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialPrologReport {
    /// Stable declared finite query scope.
    pub scope: &'static str,
    /// Per-query answer counts, hashes, and evidence locations.
    pub fixtures: Vec<SpatialPrologFixture>,
    /// Total normalized rows across all fixtures.
    pub answer_count: usize,
    /// Length-framed digest of goal names and fixture digests.
    pub corpus_digest: String,
    /// Rows retaining stable `because/2` explanations.
    pub explanation_rows: usize,
    /// Named and drag rows that agree on the valid `play(c0)` intent.
    pub equivalent_play_resolutions: usize,
    /// Explicit ambiguous placement explanation rows.
    pub ambiguous_explanations: usize,
}

/// Native execution, protocol, corpus, or digest failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialPrologError(String);

impl fmt::Display for SpatialPrologError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for SpatialPrologError {}

/// Run every finite spatial query and retain normalized answer counts/digests.
///
/// # Errors
///
/// Fails closed on unavailable Scryer, malformed/missing/duplicate answers,
/// changed bounded counts, privacy leakage, or absent valid/ambiguous reasons.
pub fn check_spatial_prolog_query_micro(
    root: &Path,
) -> Result<SpatialPrologReport, SpatialPrologError> {
    let mut fixtures = Vec::with_capacity(FIXTURES.len());
    let mut total = 0;
    let mut explanation_rows = 0;
    let mut equivalent_play_resolutions = 0;
    let mut ambiguous_explanations = 0;
    let mut corpus = blake3::Hasher::new();

    for (goal, expected_count) in FIXTURES {
        let fixture_id = format!("spatial-{}", goal.replace('_', "-"));
        let native = run_prolog_model_fixture(
            root,
            &fixture_id,
            Path::new(MODEL_PATH),
            "poche_spatial",
            goal,
        );
        if native.disposition != NativeDisposition::Success {
            return Err(problem(format!(
                "{goal} was {:?}: {} ({})",
                native.disposition,
                native.diagnostic,
                native.evidence_directory.display()
            )));
        }
        if native.answers.len() != expected_count {
            return Err(problem(format!(
                "{goal} returned {} rows, expected {expected_count}",
                native.answers.len()
            )));
        }
        validate_fixture(goal, &native.answers)?;
        explanation_rows += native
            .answers
            .iter()
            .filter(|row| row.contains("because(P3-SPATIAL-"))
            .count();
        if goal == "command_resolution" {
            equivalent_play_resolutions = native
                .answers
                .iter()
                .filter(|row| row.contains("allow(play(c0))"))
                .count();
        }
        if goal == "drop_explanations" {
            ambiguous_explanations = native
                .answers
                .iter()
                .filter(|row| row.contains("deny(ambiguous([deck,trump]))"))
                .count();
        }
        let digest = digest_rows(&native.answers);
        hash_part(&mut corpus, goal.as_bytes());
        hash_part(&mut corpus, digest.as_bytes());
        total += native.answers.len();
        fixtures.push(SpatialPrologFixture {
            goal,
            answer_count: native.answers.len(),
            digest,
            evidence_directory: native.evidence_directory.display().to_string(),
        });
    }
    if equivalent_play_resolutions != 2 || ambiguous_explanations != 1 {
        return Err(problem(format!(
            "resolution evidence changed: equivalent={equivalent_play_resolutions}, ambiguous={ambiguous_explanations}"
        )));
    }

    Ok(SpatialPrologReport {
        scope: SCOPE_ID,
        fixtures,
        answer_count: total,
        corpus_digest: format!("blake3:{}", corpus.finalize().to_hex()),
        explanation_rows,
        equivalent_play_resolutions,
        ambiguous_explanations,
    })
}

fn validate_fixture(goal: &str, answers: &BTreeSet<String>) -> Result<(), SpatialPrologError> {
    let required = match goal {
        "card_locations" => [
            "location(c0,hand(p0,0),owner(p0))",
            "location(c7,deck(1),concealed)",
        ],
        "attached_text" => [
            "attached(p0,face_text(c0,two_clubs),surface(c0,face),card_face,because(P3-SPATIAL-TEXT-001,exact_viewer_face_attachment))",
            "attached(spec,face_text(c3,seven_diamonds),surface(c3,face),card_face,because(P3-SPATIAL-TEXT-001,exact_viewer_face_attachment))",
        ],
        "command_resolution" => [
            "named(initial,p0,c0,allow(play(c0)),because(P3-SPATIAL-INTENT-001,visible_owned_hand_card))",
            "drag(initial,p0,c0,play_inner,allow(play(c0)),because(P3-SPATIAL-INTENT-001,visible_owned_hand_card))",
        ],
        "drop_explanations" => [
            "drop(initial,p0,c0,play_inner,allow(play(c0)),because(P3-SPATIAL-INTENT-001,visible_owned_hand_card))",
            "drop(initial,p0,c0,broad_deck_trump,deny(ambiguous([deck,trump])),because(P3-SPATIAL-DROP-004,multiple_outer_zones_implicated))",
        ],
        "layout_findings" => [
            "layout(layout2,valid,because(P3-SPATIAL-LAYOUT-001,complete_separated_two_player_layout))",
            "layout(overlap_layout,violation(overlap(deck,trump,deck_cell)),because(P3-SPATIAL-LAYOUT-002,outer_zone_intersection))",
        ],
        "successors" => [
            "successor(initial,p0,play(c0),after_p0,because(P3-SPATIAL-STEP-001,hand_to_play_endpoint))",
            "successor(after_p0,runtime,disconnect,recovery,because(P3-SPATIAL-STEP-004,recovery_preserves_committed_state))",
        ],
        "predecessors" => [
            "predecessor(after_p0,p0,play(c0),initial,because(P3-SPATIAL-STEP-001,hand_to_play_endpoint))",
            "predecessor(recovery,runtime,disconnect,after_p0,because(P3-SPATIAL-STEP-004,recovery_preserves_committed_state))",
        ],
        _ => return Err(problem(format!("unknown spatial Prolog fixture {goal}"))),
    };
    if required.iter().all(|row| answers.contains(*row)) {
        if goal == "attached_text"
            && answers.iter().any(|row| {
                row.starts_with("attached(p0,face_text(c2,")
                    || row.contains("face_text(c5,")
                    || row.contains("face_text(c7,")
            })
        {
            return Err(problem("attached_text leaked another hand or deck face"));
        }
        Ok(())
    } else {
        Err(problem(format!("{goal} omitted a required bounded answer")))
    }
}

fn digest_rows(rows: &BTreeSet<String>) -> String {
    let mut hasher = blake3::Hasher::new();
    for row in rows {
        hash_part(&mut hasher, row.as_bytes());
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn hash_part(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn problem(message: impl Into<String>) -> SpatialPrologError {
    SpatialPrologError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_prolog_query_micro_is_complete_and_reproducible() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = check_spatial_prolog_query_micro(&root)
            .expect("spatial Scryer query scope should pass");
        let repeated = check_spatial_prolog_query_micro(&root)
            .expect("spatial Scryer query scope should reproduce");
        assert_eq!(report, repeated);
        assert_eq!(report.fixtures.len(), 7);
        assert_eq!(report.answer_count, 63);
        assert_eq!(report.explanation_rows, 55);
        assert_eq!(report.equivalent_play_resolutions, 2);
        assert_eq!(report.ambiguous_explanations, 1);
    }
}

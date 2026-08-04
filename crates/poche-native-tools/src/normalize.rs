// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;

/// Alloy command kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlloyCommandKind {
    /// A `run` command seeking a satisfying instance.
    Witness,
    /// A `check` command seeking a counterexample.
    Assertion,
}

/// SAT solver outcome reported by Alloy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlloyOutcome {
    /// A satisfying instance exists.
    Sat,
    /// No satisfying instance exists in the command scope.
    Unsat,
}

/// One normalized Alloy command result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlloyCommandResult {
    /// Stable command name in the handwritten model.
    pub name: String,
    /// Witness or assertion command.
    pub kind: AlloyCommandKind,
    /// SAT/UNSAT result.
    pub outcome: AlloyOutcome,
    /// Instances produced/requested when the CLI reports an `n/m` count.
    pub instances: Option<(u32, u32)>,
    /// Exact command source (and therefore exact scope) from `receipt.json`.
    pub command_source: Option<String>,
}

impl AlloyCommandResult {
    /// Whether the result has the expected polarity for its command kind.
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(
            (self.kind, self.outcome),
            (AlloyCommandKind::Witness, AlloyOutcome::Sat)
                | (AlloyCommandKind::Assertion, AlloyOutcome::Unsat)
        )
    }
}

/// `NuSMV` result category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NuSmvPropertyKind {
    /// CTL or LTL specification (`NuSMV` uses the same output prefix for both).
    Specification,
    /// State invariant.
    Invariant,
}

/// One normalized `NuSMV` property result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NuSmvPropertyResult {
    /// Output category.
    pub kind: NuSmvPropertyKind,
    /// `NuSMV`'s normalized property expression.
    pub expression: String,
    /// Truth value reported by `NuSMV`.
    pub holds: bool,
}

/// One named Scryer Prolog corpus result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologTestResult {
    /// Stable `oracle_test/1` atom.
    pub name: String,
    /// Whether the query succeeded.
    pub passed: bool,
}

/// Backend-specific normalized evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizedRun {
    /// Alloy command outcomes and receipt-derived command scopes.
    Alloy(Vec<AlloyCommandResult>),
    /// `NuSMV` invariant/temporal results.
    NuSmv(Vec<NuSmvPropertyResult>),
    /// Scryer Prolog named query results.
    ScryerProlog(Vec<PrologTestResult>),
}

impl NormalizedRun {
    /// True when every recognized native result has its passing value.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        match self {
            Self::Alloy(results) => results.iter().all(AlloyCommandResult::passed),
            Self::NuSmv(results) => results.iter().all(|result| result.holds),
            Self::ScryerProlog(results) => results.iter().all(|result| result.passed),
        }
    }

    /// Number of normalized command/property/query results.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Alloy(results) => results.len(),
            Self::NuSmv(results) => results.len(),
            Self::ScryerProlog(results) => results.len(),
        }
    }

    /// Whether no native results were recognized.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub(crate) const ALLOY_WITNESSES: [&str; 7] = [
    "CompleteRoundWitness",
    "UnrestrictedBidWitness",
    "ZeroBidSuccessWitness",
    "SharedWinnerWitness",
    "FirstJackWitness",
    "RepeatedHighCardWitness",
    "ParameterBoundaryWitness",
];

pub(crate) const ALLOY_ASSERTIONS: [&str; 8] = [
    "CompleteDeckIsExactly52",
    "CardConservationAndPartition",
    "FollowSuitIsEnforced",
    "DealerBidsLastInClockwiseOrder",
    "WinnerIsEligibleAndHighest",
    "ScoreAndPaymentAgree",
    "ScheduleBoundariesAndFeasibility",
    "FinalWinnersAreExactlyTheMaxima",
];

pub(crate) const PROLOG_TESTS: [&str; 16] = [
    "standard_deck",
    "all_player_schedules",
    "score_sheet_rotation",
    "generic_deal",
    "bid_domain",
    "follow_suit",
    "void_play",
    "trump_winner",
    "lead_winner",
    "scoring_and_reverse_scoring",
    "money_and_shared_winners",
    "first_jack_selection",
    "repeated_high_card",
    "forward_round_trace",
    "reverse_bid_predecessor",
    "reverse_play_predecessor",
];

/// Parse Alloy CLI output. Any missing, duplicate, or extra command is unknown.
///
/// # Errors
///
/// Returns a diagnostic when any required command is absent or when a result
/// line contains an unknown name, kind, outcome, or duplicate.
pub fn normalize_alloy(transcript: &str) -> Result<NormalizedRun, String> {
    let cleaned: String = transcript
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect();
    let expected: BTreeSet<&str> = ALLOY_WITNESSES
        .into_iter()
        .chain(ALLOY_ASSERTIONS)
        .collect();
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();

    for line in cleaned.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(kind_index) = tokens
            .iter()
            .position(|token| matches!(*token, "run" | "check"))
        else {
            continue;
        };
        let Some(name) = tokens.get(kind_index + 1).copied() else {
            return Err(format!("malformed Alloy result line: {line}"));
        };
        if !expected.contains(name) {
            return Err(format!("unknown Alloy command in output: {name}"));
        }
        if !seen.insert(name) {
            return Err(format!("duplicate Alloy command result: {name}"));
        }
        let kind = match tokens[kind_index] {
            "run" => AlloyCommandKind::Witness,
            "check" => AlloyCommandKind::Assertion,
            _ => unreachable!("position accepts only run/check"),
        };
        let outcome = match tokens.last().copied() {
            Some("SAT") => AlloyOutcome::Sat,
            Some("UNSAT") => AlloyOutcome::Unsat,
            _ => return Err(format!("unknown Alloy outcome for {name}: {line}")),
        };
        let instances = tokens.iter().find_map(|token| {
            let (found, requested) = token.split_once('/')?;
            Some((found.parse().ok()?, requested.parse().ok()?))
        });
        results.push(AlloyCommandResult {
            name: name.to_owned(),
            kind,
            outcome,
            instances,
            command_source: None,
        });
    }

    let missing: Vec<_> = expected.difference(&seen).copied().collect();
    if !missing.is_empty() {
        return Err(format!(
            "Alloy output omitted commands: {}",
            missing.join(", ")
        ));
    }
    Ok(NormalizedRun::Alloy(results))
}

/// Parse `NuSMV`'s stable result-line grammar.
///
/// # Errors
///
/// Returns a diagnostic when there are no results or a result line has an
/// empty expression, malformed separator, or unknown truth value.
pub fn normalize_nusmv(transcript: &str) -> Result<NormalizedRun, String> {
    let mut results = Vec::new();
    for line in transcript.lines() {
        let line = line.trim();
        let parsed = if let Some(rest) = line.strip_prefix("-- specification") {
            Some((NuSmvPropertyKind::Specification, rest))
        } else {
            line.strip_prefix("-- invariant")
                .map(|rest| (NuSmvPropertyKind::Invariant, rest))
        };
        let Some((kind, rest)) = parsed else {
            continue;
        };
        let Some((expression, value)) = rest.rsplit_once(" is ") else {
            return Err(format!("malformed NuSMV result line: {line}"));
        };
        let holds = match value.trim() {
            "true" => true,
            "false" => false,
            other => return Err(format!("unknown NuSMV truth value `{other}`")),
        };
        let expression = expression.trim();
        if expression.is_empty() {
            return Err("NuSMV emitted an empty property expression".to_owned());
        }
        results.push(NuSmvPropertyResult {
            kind,
            expression: expression.to_owned(),
            holds,
        });
    }
    if results.is_empty() {
        return Err("NuSMV output contained no property results".to_owned());
    }
    Ok(NormalizedRun::NuSmv(results))
}

/// Parse the named Scryer Prolog corpus protocol.
///
/// # Errors
///
/// Returns a diagnostic when a test is missing, duplicated, unknown, malformed,
/// or inconsistent with the aggregate count.
pub fn normalize_prolog(transcript: &str) -> Result<NormalizedRun, String> {
    let expected: BTreeSet<&str> = PROLOG_TESTS.into_iter().collect();
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    let mut declared_count = None;

    for line in transcript.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("POCHE_PROLOG_TEST ") {
            let Some((name, status)) = rest.split_once(' ') else {
                return Err(format!("malformed Prolog test result: {line}"));
            };
            if !expected.contains(name) {
                return Err(format!("unknown Prolog oracle test: {name}"));
            }
            if !seen.insert(name) {
                return Err(format!("duplicate Prolog oracle test: {name}"));
            }
            let passed = match status.trim() {
                "PASS" => true,
                "FAIL" => false,
                other => return Err(format!("unknown Prolog test status `{other}`")),
            };
            results.push(PrologTestResult {
                name: name.to_owned(),
                passed,
            });
        } else if let Some(count) = line.strip_prefix("POCHE_PROLOG_OK tests=") {
            declared_count = Some(
                count
                    .parse::<usize>()
                    .map_err(|_| format!("invalid Prolog test count `{count}`"))?,
            );
        }
    }

    let missing: Vec<_> = expected.difference(&seen).copied().collect();
    if !missing.is_empty() {
        return Err(format!(
            "Prolog output omitted tests: {}",
            missing.join(", ")
        ));
    }
    if declared_count != Some(results.len()) {
        return Err(format!(
            "Prolog aggregate count {declared_count:?} disagrees with {} named results",
            results.len()
        ));
    }
    Ok(NormalizedRun::ScryerProlog(results))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn alloy_parser_accepts_exact_corpus_and_control_progress() {
        let mut transcript = String::new();
        for (index, name) in ALLOY_WITNESSES.iter().enumerate() {
            writeln!(transcript, "{index:02}. run {name} 0\u{8}\u{8} 1/1 SAT")
                .expect("writing to String cannot fail");
        }
        for (offset, name) in ALLOY_ASSERTIONS.iter().enumerate() {
            writeln!(
                transcript,
                "{:02}. check {name} 0 UNSAT",
                offset + ALLOY_WITNESSES.len()
            )
            .expect("writing to String cannot fail");
        }
        let result = normalize_alloy(&transcript).expect("known transcript parses");
        assert_eq!(result.len(), 15);
        assert!(result.all_passed());
    }

    #[test]
    fn alloy_parser_fails_closed_on_unknown_or_missing_output() {
        assert!(normalize_alloy("00. run Surprise 1/1 SAT\n").is_err());
        assert!(normalize_alloy("All done\n").is_err());
    }

    #[test]
    fn nusmv_parser_distinguishes_failure_and_unknown() {
        let result = normalize_nusmv(
            "-- specification AF phase = finished  is true\n-- invariant card_total = 52  is false\n",
        )
        .expect("known true/false grammar parses");
        assert_eq!(result.len(), 2);
        assert!(!result.all_passed());
        assert!(normalize_nusmv("-- specification X is maybe\n").is_err());
    }

    #[test]
    fn prolog_parser_requires_every_named_result_and_aggregate() {
        let mut transcript = String::new();
        for name in PROLOG_TESTS {
            writeln!(transcript, "POCHE_PROLOG_TEST {name} PASS")
                .expect("writing to String cannot fail");
        }
        transcript.push_str("POCHE_PROLOG_OK tests=16\n");
        let result = normalize_prolog(&transcript).expect("known transcript parses");
        assert_eq!(result.len(), 16);
        assert!(result.all_passed());
        assert!(normalize_prolog("POCHE_PROLOG_OK tests=16\n").is_err());
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Fail-closed runners and normalized results for the handwritten native oracles.
//!
//! This crate deliberately does not generate Alloy, `NuSMV`, or Prolog source.
//! It runs those independent models, retains their native evidence, and maps
//! stable common fixtures to stable selectors in each language.

mod adapters;
mod normalize;
mod runner;

use std::collections::BTreeSet;
use std::path::PathBuf;

pub use adapters::{
    FixtureAdapter, FixtureEvaluation, FixtureKind, NativeSelector, SelectorEvaluation,
    evaluate_fixtures, fixture_adapters,
};
pub use normalize::{
    AlloyCommandKind, AlloyCommandResult, AlloyOutcome, NormalizedRun, NuSmvPropertyKind,
    NuSmvPropertyResult, PrologTestResult, normalize_alloy, normalize_nusmv, normalize_prolog,
};
pub use runner::{run_all, run_backend, run_prolog_fixture};

/// A native backend whose handwritten model can be executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeBackend {
    /// Alloy 6 relational model and bounded commands.
    Alloy,
    /// `NuSMV` symbolic transition system.
    NuSmv,
    /// Scryer Prolog relational program.
    ScryerProlog,
}

impl NativeBackend {
    /// Stable CLI/display name.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Alloy => "alloy",
            Self::NuSmv => "nusmv",
            Self::ScryerProlog => "prolog",
        }
    }

    /// Ignored evidence directory below the workspace target directory.
    #[must_use]
    pub const fn evidence_subdirectory(self) -> &'static str {
        match self {
            Self::Alloy => "alloy-oracle",
            Self::NuSmv => "nusmv-oracle",
            Self::ScryerProlog => "prolog-oracle",
        }
    }
}

/// Top-level interpretation of a native invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeDisposition {
    /// The transcript was recognized and every required result passed.
    Success,
    /// The transcript was recognized but the tool or a property reported failure.
    Failure,
    /// The process succeeded but its output did not match the accepted grammar.
    Unknown,
}

/// Raw subprocess evidence retained independently of normalized results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawInvocation {
    /// Resolved executable.
    pub program: String,
    /// Exact native arguments, losslessly represented for the current models.
    pub arguments: Vec<String>,
    /// Version probe summary.
    pub version: String,
    /// Process exit code, or `None` when Windows did not provide one.
    pub exit_code: Option<i32>,
    /// Exact stdout bytes decoded lossily for diagnostics; raw bytes are on disk.
    pub stdout: String,
    /// Exact stderr bytes decoded lossily for diagnostics; raw bytes are on disk.
    pub stderr: String,
}

/// Complete typed outcome of one native backend invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReport {
    /// Backend that was requested.
    pub backend: NativeBackend,
    /// Success, recognized failure, or unknown output.
    pub disposition: NativeDisposition,
    /// Human-readable typed diagnostic.
    pub diagnostic: String,
    /// Raw invocation when the executable launched.
    pub raw: Option<RawInvocation>,
    /// Parsed backend-specific evidence when parsing succeeded.
    pub normalized: Option<NormalizedRun>,
    /// Directory containing raw and normalized ignored evidence.
    pub evidence_directory: PathBuf,
}

impl NativeReport {
    /// Whether this report is a recognized all-passing result.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.disposition == NativeDisposition::Success
    }
}

/// Typed result of one grounded Scryer Prolog cross-model fixture query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrologFixtureReport {
    /// Stable fixture ID used for evidence paths.
    pub fixture_id: String,
    /// Native goal atom accepted by `run_conformance_fixture/1`.
    pub native_goal: String,
    /// Success, process/query failure, or unrecognized output.
    pub disposition: NativeDisposition,
    /// Human-readable diagnostic.
    pub diagnostic: String,
    /// Order-independent, duplicate-free normalized answer rows.
    pub answers: BTreeSet<String>,
    /// Raw native invocation when Scryer launched.
    pub raw: Option<RawInvocation>,
    /// Ignored per-fixture evidence directory.
    pub evidence_directory: PathBuf,
}

impl PrologFixtureReport {
    /// Whether the process and strict answer protocol both succeeded.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.disposition == NativeDisposition::Success
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Semantic multi-device puppet harness for Poche.
//!
//! Puppets are ordinary certified devices. They select opaque actions from
//! exact-recipient observations and have no local-process control or reducer
//! mutation port.

mod artifacts;
mod catalog;
mod scenario;

use core::fmt;
use std::{path::PathBuf, time::Duration};

pub use catalog::{ScenarioDescriptor, TWO_PLAYER_FULL_ROUND, scenario, scenarios};
pub use scenario::{
    DeviceRevisionEvidence, PuppetDeviceEvidence, PuppetRunReport, PuppetStepEvidence,
};

/// Supported renderer/presentation targets for a puppet scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuppetSurface {
    Headless,
}

/// Runtime transport selected by the scenario. Both loopback variants enter
/// the same reducer; NDJSON additionally exercises canonical framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuppetTransport {
    LoopbackTyped,
    LoopbackNdjson,
}

impl PuppetTransport {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LoopbackTyped => "loopback-typed",
            Self::LoopbackNdjson => "loopback-ndjson",
        }
    }
}

impl PuppetSurface {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Headless => "headless",
        }
    }
}

/// Bounded run configuration. Deadlines are checked between semantic actions;
/// the harness never sleeps to guess whether state has changed.
#[derive(Clone, Debug)]
pub struct PuppetRunOptions {
    pub scenario: String,
    pub surface: PuppetSurface,
    pub transport: PuppetTransport,
    pub seed: u64,
    pub max_steps: u32,
    pub per_action_timeout: Duration,
    pub whole_run_timeout: Duration,
    pub artifact_root: PathBuf,
}

impl Default for PuppetRunOptions {
    fn default() -> Self {
        Self {
            scenario: TWO_PLAYER_FULL_ROUND.to_owned(),
            surface: PuppetSurface::Headless,
            transport: PuppetTransport::LoopbackTyped,
            seed: 1,
            max_steps: 2_000,
            per_action_timeout: Duration::from_secs(5),
            whole_run_timeout: Duration::from_secs(30),
            artifact_root: default_artifact_root(),
        }
    }
}

#[must_use]
pub fn default_artifact_root() -> PathBuf {
    PathBuf::from("target").join("poche-puppets")
}

/// Run a catalog scenario with a caller-owned cancellation probe.
///
/// # Errors
///
/// Returns a stable error if the scenario/surface is unsupported, a device
/// violates the protocol, a semantic deadline is exceeded, cancellation is
/// requested, or evidence cannot be persisted atomically.
pub fn run_with_cancel(
    options: &PuppetRunOptions,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PuppetRunReport, PuppetError> {
    if scenario(&options.scenario).is_none() {
        return Err(PuppetError::new(
            PuppetErrorCode::UnknownScenario,
            "puppet scenario is not in the static catalog",
        ));
    }
    if options.surface != PuppetSurface::Headless {
        return Err(PuppetError::new(
            PuppetErrorCode::UnsupportedSurface,
            "puppet surface is not implemented for this scenario",
        ));
    }
    let report = scenario::run_two_player_full_round(options, &mut cancelled)?;
    artifacts::persist_run(options, report)
}

/// Run a catalog scenario without external cancellation.
///
/// # Errors
///
/// Returns the same stable failures as [`run_with_cancel`].
pub fn run(options: &PuppetRunOptions) -> Result<PuppetRunReport, PuppetError> {
    run_with_cancel(options, || false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuppetErrorCode {
    UnknownScenario,
    UnsupportedSurface,
    InvalidFixture,
    DeviceProtocol,
    ActionDenied,
    NoAction,
    StepLimit,
    ActionTimeout,
    RunTimeout,
    Cancelled,
    EvidenceIo,
}

impl PuppetErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownScenario => "PUPPET-UNKNOWN-SCENARIO",
            Self::UnsupportedSurface => "PUPPET-UNSUPPORTED-SURFACE",
            Self::InvalidFixture => "PUPPET-INVALID-FIXTURE",
            Self::DeviceProtocol => "PUPPET-DEVICE-PROTOCOL",
            Self::ActionDenied => "PUPPET-ACTION-DENIED",
            Self::NoAction => "PUPPET-NO-ACTION",
            Self::StepLimit => "PUPPET-STEP-LIMIT",
            Self::ActionTimeout => "PUPPET-ACTION-TIMEOUT",
            Self::RunTimeout => "PUPPET-RUN-TIMEOUT",
            Self::Cancelled => "PUPPET-CANCELLED",
            Self::EvidenceIo => "PUPPET-EVIDENCE-IO",
        }
    }
}

#[derive(Debug)]
pub struct PuppetError {
    code: PuppetErrorCode,
    message: &'static str,
}

impl PuppetError {
    #[must_use]
    pub const fn new(code: PuppetErrorCode, message: &'static str) -> Self {
        Self { code, message }
    }

    #[must_use]
    pub const fn code(&self) -> PuppetErrorCode {
        self.code
    }
}

impl fmt::Display for PuppetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for PuppetError {}

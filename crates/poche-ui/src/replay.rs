// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_runtime::GoldenTranscript;

use crate::{ConnectionPresentation, PresentationInput, PresentationModel};

/// One viewer checkpoint extracted from a verified transcript fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayCheckpoint {
    pub step_index: usize,
    pub viewer: String,
    pub authorization: String,
    pub outcome: String,
    pub events: Vec<String>,
    pub presentation: PresentationModel,
}

/// Static replay deck shared by native egui and browser/WASM clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayDeck {
    pub fixture_id: String,
    pub source_steps: usize,
    pub checkpoints: Vec<ReplayCheckpoint>,
}

impl ReplayDeck {
    /// Decode viewer checkpoints from canonical transcript JSON.
    ///
    /// # Errors
    ///
    /// Returns a strict JSON error or rejects a fixture with no scoped views.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let transcript: GoldenTranscript = serde_json::from_str(json)
            .map_err(|error| format!("invalid replay fixture: {error}"))?;
        let mut checkpoints = Vec::new();
        for (step_index, step) in transcript.steps.iter().enumerate() {
            for (viewer, projection) in &step.scoped_projections {
                checkpoints.push(ReplayCheckpoint {
                    step_index,
                    viewer: viewer.clone(),
                    authorization: step.authorization.clone(),
                    outcome: step.outcome.clone(),
                    events: step.events.clone(),
                    presentation: PresentationModel::from_input(PresentationInput {
                        viewer: viewer.clone(),
                        projection: projection.clone(),
                        legal_actions: Vec::new(),
                        connection: ConnectionPresentation::Replay,
                        countdown: None,
                        chat: Vec::new(),
                        notices: Vec::new(),
                    }),
                });
            }
        }
        if checkpoints.is_empty() {
            return Err("replay fixture has no viewer-scoped checkpoints".to_owned());
        }
        Ok(Self {
            fixture_id: transcript.fixture_id,
            source_steps: transcript.steps.len(),
            checkpoints,
        })
    }

    /// Index of the first checkpoint for `viewer`, when present.
    #[must_use]
    pub fn first_for_viewer(&self, viewer: &str) -> Option<usize> {
        self.checkpoints
            .iter()
            .position(|checkpoint| checkpoint.viewer == viewer)
    }
}

#[cfg(test)]
mod tests {
    use crate::{EMBEDDED_REPLAY, ReplayDeck};

    #[test]
    fn embedded_fixture_exposes_multiple_exact_viewers_and_phases() {
        let deck = ReplayDeck::from_json(EMBEDDED_REPLAY).expect("checked fixture");
        assert_eq!(deck.fixture_id, "micro-session-v1");
        assert!(deck.first_for_viewer("host").is_some());
        assert!(deck.first_for_viewer("alice").is_some());
        assert!(deck.first_for_viewer("bob").is_some());
        assert!(deck.checkpoints.iter().any(|frame| {
            frame.viewer == "bob" && !frame.presentation.granted_hands.is_empty()
        }));
        assert!(deck.checkpoints.iter().any(|frame| {
            frame.viewer == "bob"
                && frame.step_index > 20
                && frame.presentation.granted_hands.is_empty()
        }));
    }

    #[test]
    fn empty_or_unscoped_fixture_fails_closed() {
        assert!(ReplayDeck::from_json("{}").is_err());
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic presentation models and an egui replay client.
//!
//! This crate deliberately consumes only viewer-scoped protocol projections.
//! It cannot inspect the authoritative session state and therefore cannot
//! accidentally turn UI filtering into the private-hand security boundary.

mod presentation;
mod replay;
mod semantic_html;
#[cfg(feature = "egui")]
mod widgets;

pub use presentation::*;
pub use replay::*;
pub use semantic_html::*;
#[cfg(feature = "egui")]
pub use widgets::PocheReplayApp;

/// Checked, viewer-scoped fixture embedded into native and web replay clients.
pub const EMBEDDED_REPLAY: &str =
    include_str!("../../../tests/fixtures/protocol/session-micro-v1.json");

/// Browser handle for the static replay/demo.
#[cfg(all(target_arch = "wasm32", feature = "egui"))]
#[derive(Clone)]
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct WebHandle {
    runner: eframe::WebRunner,
}

#[cfg(all(target_arch = "wasm32", feature = "egui"))]
#[wasm_bindgen::prelude::wasm_bindgen]
impl WebHandle {
    /// Create a stopped browser replay runner.
    #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    /// Start the static fixture replay on `canvas`.
    ///
    /// # Errors
    ///
    /// Returns JavaScript startup or rendering failures.
    pub async fn start(
        &self,
        canvas: web_sys::HtmlCanvasElement,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_| {
                    Ok(Box::new(
                        PocheReplayApp::from_json(EMBEDDED_REPLAY)
                            .expect("the checked replay fixture must decode"),
                    ))
                }),
            )
            .await
    }

    /// Stop the browser runner and release its event handlers.
    pub fn destroy(&self) {
        self.runner.destroy();
    }

    /// Report whether the eframe runner trapped a panic.
    #[must_use]
    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }
}

#[cfg(all(target_arch = "wasm32", feature = "egui"))]
impl Default for WebHandle {
    fn default() -> Self {
        Self::new()
    }
}

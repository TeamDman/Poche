// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_ui::PocheReplayApp;

fn main() -> eframe::Result {
    eframe::run_native(
        "Poche projection replay",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(PocheReplayApp::embedded()))),
    )
}

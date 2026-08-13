// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::Serialize;

/// One static scenario description. Catalog inspection never starts a
/// renderer, transport, or authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ScenarioDescriptor {
    pub schema: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub surfaces: &'static [&'static str],
    pub transports: &'static [&'static str],
    pub player_roots: u8,
    pub devices: u8,
    pub completion: &'static str,
}

pub const TWO_PLAYER_FULL_ROUND: &str = "two-player-full-round";

const SCENARIOS: [ScenarioDescriptor; 1] = [ScenarioDescriptor {
    schema: "poche.puppet.scenario.v1",
    name: TWO_PLAYER_FULL_ROUND,
    description: "Create a two-player room and complete the full deterministic Poche schedule through certified devices.",
    surfaces: &["headless", "native"],
    transports: &["loopback-typed", "loopback-ndjson"],
    player_roots: 3,
    devices: 7,
    completion: "room reaches post_game and every enrolled device observes the terminal revision",
}];

#[must_use]
pub const fn scenarios() -> &'static [ScenarioDescriptor] {
    &SCENARIOS
}

#[must_use]
pub fn scenario(name: &str) -> Option<&'static ScenarioDescriptor> {
    SCENARIOS.iter().find(|scenario| scenario.name == name)
}

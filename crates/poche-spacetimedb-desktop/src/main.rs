// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

fn main() {
    if let Err(error) = poche_spacetimedb_desktop::run_from_env() {
        eprintln!("poche: {error}");
        std::process::exit(2);
    }
}

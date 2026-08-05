// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;

use poche_burn::{TrainingRunManifest, evaluate, train};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 || arguments[1] != "--manifest" {
        eprintln!("usage: poche-burn-run train|evaluate --manifest <path>");
        std::process::exit(2);
    }
    let manifest = match TrainingRunManifest::load(Path::new(&arguments[2])) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("invalid training manifest: {error:?}");
            std::process::exit(1);
        }
    };
    let encoded = match arguments[0].as_str() {
        "train" => train(&manifest).and_then(|summary| {
            serde_json::to_string_pretty(&summary).map_err(|_| poche_burn::TrainingError::Manifest)
        }),
        "evaluate" => evaluate(&manifest).and_then(|summary| {
            serde_json::to_string_pretty(&summary).map_err(|_| poche_burn::TrainingError::Manifest)
        }),
        _ => {
            eprintln!("usage: poche-burn-run train|evaluate --manifest <path>");
            std::process::exit(2);
        }
    };
    match encoded {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("learning command failed: {error:?}");
            std::process::exit(1);
        }
    }
}

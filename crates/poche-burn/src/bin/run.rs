// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;

use poche_burn::{TrainingRunManifest, evaluate, replay_selected, train};

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() < 3 || arguments[1] != "--manifest" {
        usage();
        std::process::exit(2);
    }
    let manifest = match TrainingRunManifest::load(Path::new(&arguments[2])) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("invalid training manifest: {error:?}");
            std::process::exit(1);
        }
    };
    let encoded = match (arguments[0].as_str(), arguments.as_slice()) {
        ("train", [_, _, _]) => train(&manifest).and_then(|summary| {
            serde_json::to_string_pretty(&summary).map_err(|_| poche_burn::TrainingError::Manifest)
        }),
        ("evaluate", [_, _, _]) => evaluate(&manifest).and_then(|summary| {
            serde_json::to_string_pretty(&summary).map_err(|_| poche_burn::TrainingError::Manifest)
        }),
        ("replay", [_, _, _, matchup_flag, matchup, seed_flag, seed])
            if matchup_flag == "--matchup" && seed_flag == "--seed" =>
        {
            let seed = seed
                .parse::<u64>()
                .map_err(|_| poche_burn::TrainingError::Manifest);
            seed.and_then(|seed| replay_selected(&manifest, matchup, seed))
                .and_then(|episode| {
                    let hash = episode
                        .semantic_hash()
                        .map_err(|_| poche_burn::TrainingError::Evaluation)?;
                    eprintln!("episode_semantic_hash={hash}");
                    episode
                        .ndjson()
                        .map(|ndjson| ndjson.trim_end_matches('\n').to_owned())
                        .map_err(|_| poche_burn::TrainingError::Evaluation)
                })
        }
        _ => {
            usage();
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

fn usage() {
    eprintln!(
        "usage: poche-burn-run train|evaluate --manifest <path>\n       poche-burn-run replay --manifest <path> --matchup <name> --seed <seed>"
    );
}

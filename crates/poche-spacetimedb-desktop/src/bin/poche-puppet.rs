// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Disposable two-device acceptance puppet plus an ad-hoc client for a live
//! Poche file-control endpoint.

#![allow(clippy::needless_pass_by_value, clippy::too_many_lines)]

use image::{Rgba, RgbaImage, imageops};
use poche_spacetimedb_desktop::AuthorityEndpoint;
use poche_spacetimedb_desktop::file_control::{
    FileControlAction, FileControlObservation, FileControlResponse, FileControlStatus,
    send_file_control_request,
};
use serde::Serialize;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const TIMEOUT: Duration = Duration::from_secs(25);

fn main() {
    if let Err(error) = run() {
        eprintln!("poche-puppet: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("acceptance") => {
            let mut output = PathBuf::from("target/poche-puppet/acceptance-contact-sheet.png");
            let mut server = None;
            let mut database = None;
            while let Some(argument) = args.next() {
                match argument.as_str() {
                    "--output" => {
                        output = PathBuf::from(args.next().ok_or("--output requires a path")?);
                    }
                    "--server" => {
                        server = Some(args.next().ok_or("--server requires a value")?);
                    }
                    "--database" => {
                        database = Some(args.next().ok_or("--database requires a value")?);
                    }
                    unknown => return Err(format!("unknown acceptance option {unknown:?}")),
                }
            }
            let authority = AuthorityEndpoint::select(server.as_deref(), database.as_deref())?;
            acceptance(&absolute(output)?, &authority)
        }
        Some("observe") => {
            let root = PathBuf::from(args.next().ok_or("observe requires CONTROL_ROOT")?);
            let include_join_code = args.next().as_deref() == Some("--include-join-code");
            print_response(send(
                &root,
                FileControlAction::Observe { include_join_code },
            )?)
        }
        Some("set-name") => {
            let root = PathBuf::from(args.next().ok_or("set-name requires CONTROL_ROOT")?);
            let name = args.next().ok_or("set-name requires NAME")?;
            print_response(send(&root, FileControlAction::SetName { name })?)
        }
        Some("create") => {
            let root = PathBuf::from(args.next().ok_or("create requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::CreateLobby)?)
        }
        Some("join") => {
            let root = PathBuf::from(args.next().ok_or("join requires CONTROL_ROOT")?);
            let join_code = args.next().ok_or("join requires JOIN_CODE")?;
            print_response(send(&root, FileControlAction::JoinLobby { join_code })?)
        }
        Some("seat") => {
            let root = PathBuf::from(args.next().ok_or("seat requires CONTROL_ROOT")?);
            let seat = args
                .next()
                .ok_or("seat requires zero-based SEAT")?
                .parse::<u8>()
                .map_err(|error| format!("invalid seat: {error}"))?;
            print_response(send(&root, FileControlAction::TakeSeat { seat })?)
        }
        Some("stand") => {
            let root = PathBuf::from(args.next().ok_or("stand requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ReleaseSeat)?)
        }
        Some("move") => {
            let root = PathBuf::from(args.next().ok_or("move requires CONTROL_ROOT")?);
            let card_index = parse(&mut args, "CARD_INDEX")?;
            let x = parse(&mut args, "X_MM")?;
            let y = parse(&mut args, "Y_MM")?;
            let z = parse(&mut args, "Z_MM")?;
            let rz = parse(&mut args, "RZ_MDEG")?;
            print_response(send(
                &root,
                FileControlAction::MoveOwnCard {
                    card_index,
                    position_mm: [x, y, z],
                    rotation_mdeg: [0, 0, rz],
                },
            )?)
        }
        Some("capture") => {
            let root = PathBuf::from(args.next().ok_or("capture requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::Capture)?)
        }
        Some("stop") => {
            let root = PathBuf::from(args.next().ok_or("stop requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::Stop)?)
        }
        Some("--help" | "-h") => {
            println!(
                "poche-puppet acceptance [--server local|maincloud|URL] [--database NAME]\n\
                 \x20                       [--output PATH]\n\
                 poche-puppet observe ROOT [--include-join-code]\n\
                 poche-puppet set-name ROOT NAME | create ROOT | join ROOT CODE\n\
                 poche-puppet seat ROOT 0|1 | stand ROOT\n\
                 poche-puppet move ROOT CARD_INDEX X_MM Y_MM Z_MM RZ_MDEG\n\
                 poche-puppet capture ROOT | stop ROOT"
            );
            Ok(())
        }
        Some(unknown) => Err(format!("unknown command {unknown:?}")),
    }
}

fn parse<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    label: &str,
) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    args.next()
        .ok_or_else(|| format!("move requires {label}"))?
        .parse()
        .map_err(|error| format!("invalid {label}: {error}"))
}

fn send(root: &Path, action: FileControlAction) -> Result<FileControlResponse, String> {
    let response = send_file_control_request(root, action, Some(TIMEOUT))?;
    if response.status == FileControlStatus::Rejected {
        return Err(response
            .error
            .clone()
            .unwrap_or_else(|| "request rejected".into()));
    }
    Ok(response)
}

fn print_response(response: FileControlResponse) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&response)
            .map_err(|error| format!("could not encode response: {error}"))?
    );
    Ok(())
}

#[derive(Serialize)]
struct AcceptanceReport {
    schema: &'static str,
    completed_unix_ms: u64,
    authority_profile: String,
    authority_uri: String,
    authority_database: String,
    room_id: String,
    alice_identity: String,
    bob_identity: String,
    alice_private_cards: usize,
    bob_private_cards: usize,
    shared_face_free_poses: usize,
    moved_card_key: String,
    moved_position_mm: [i32; 3],
    move_authority_ms: f64,
    move_peer_observation_ms: f64,
    screenshots: Vec<String>,
    contact_sheet: String,
}

fn acceptance(output: &Path, authority: &AuthorityEndpoint) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate puppet executable: {error}"))?
        .parent()
        .ok_or("puppet executable has no parent")?
        .join(if cfg!(windows) { "poche.exe" } else { "poche" });
    if !executable.exists() {
        return Err(format!(
            "{} is missing; run `cargo build -p poche-spacetimedb-desktop --bins` first",
            executable.display()
        ));
    }
    let parent = output.parent().ok_or("output path has no parent")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create output directory: {error}"))?;
    let run_id = format!("{}-{}", std::process::id(), unix_millis()?);
    let run_root = parent.join(format!("run-{run_id}"));
    fs::create_dir(&run_root)
        .map_err(|error| format!("could not create puppet run root: {error}"))?;
    let alice_root = run_root.join("alice");
    let bob_root = run_root.join("bob");
    let mut alice = spawn_device(
        &executable,
        &alice_root,
        "alice-window",
        &run_root,
        authority,
    )?;
    let mut bob = spawn_device(&executable, &bob_root, "bob-window", &run_root, authority)?;

    let result = acceptance_inner(output, &run_id, &alice_root, &bob_root, authority);
    stop_and_wait(&alice_root, &mut alice);
    stop_and_wait(&bob_root, &mut bob);
    result
}

fn acceptance_inner(
    output: &Path,
    run_id: &str,
    alice_root: &Path,
    bob_root: &Path,
    authority: &AuthorityEndpoint,
) -> Result<(), String> {
    wait_for_descriptor(alice_root)?;
    wait_for_descriptor(bob_root)?;
    let short_run = &run_id[..run_id.len().min(12)];
    send(
        alice_root,
        FileControlAction::SetName {
            name: format!("Alice {short_run}"),
        },
    )?;
    send(
        bob_root,
        FileControlAction::SetName {
            name: format!("Bob {short_run}"),
        },
    )?;
    send(alice_root, FileControlAction::CreateLobby)?;
    let created = send(
        alice_root,
        FileControlAction::Observe {
            include_join_code: true,
        },
    )?;
    let join_code = created
        .observation
        .join_code
        .clone()
        .ok_or("creator observation omitted the requested join code")?;
    send(bob_root, FileControlAction::JoinLobby { join_code })?;
    send(alice_root, FileControlAction::TakeSeat { seat: 0 })?;
    send(bob_root, FileControlAction::TakeSeat { seat: 1 })?;
    let alice_ready = wait_until(alice_root, |observation| observation.own_hand.len() == 5)?;
    let bob_ready = wait_until(bob_root, |observation| observation.own_hand.len() == 5)?;

    let seated_alice = capture(alice_root)?;
    let seated_bob = capture(bob_root)?;
    let moved_position = [650, 160, -250];
    let moved_rotation = [0, 0, 18_000];
    let mut owned_keys = alice_ready
        .own_hand
        .iter()
        .map(|card| card.card_key.clone())
        .collect::<Vec<_>>();
    owned_keys.sort();
    let moved_key = owned_keys
        .first()
        .ok_or("Alice has no private card")?
        .clone();
    let started = Instant::now();
    let moved = send(
        alice_root,
        FileControlAction::MoveOwnCard {
            card_index: 0,
            position_mm: moved_position,
            rotation_mdeg: moved_rotation,
        },
    )?;
    let authority_ms = moved.request_elapsed_ms;
    let bob_after = wait_until(bob_root, |observation| {
        observation.card_poses.iter().any(|pose| {
            pose.card_key == moved_key
                && pose.position_mm == moved_position
                && pose.rotation_mdeg == moved_rotation
        })
    })?;
    let peer_ms = started.elapsed().as_secs_f64() * 1_000.;
    let moved_alice = capture(alice_root)?;
    let moved_bob = capture(bob_root)?;
    let captures = [seated_alice, seated_bob, moved_alice, moved_bob];
    compose_contact_sheet(&captures, output)?;

    let report = AcceptanceReport {
        schema: "poche-spacetimedb-two-device-acceptance-v2",
        completed_unix_ms: unix_millis()?,
        authority_profile: authority.profile.clone(),
        authority_uri: authority.uri.clone(),
        authority_database: authority.database.clone(),
        room_id: alice_ready.room_id.clone().ok_or("Alice omitted room id")?,
        alice_identity: alice_ready
            .viewer_identity
            .clone()
            .ok_or("Alice omitted identity")?,
        bob_identity: bob_ready
            .viewer_identity
            .clone()
            .ok_or("Bob omitted identity")?,
        alice_private_cards: alice_ready.own_hand.len(),
        bob_private_cards: bob_ready.own_hand.len(),
        shared_face_free_poses: bob_after.card_poses.len(),
        moved_card_key: moved_key,
        moved_position_mm: moved_position,
        move_authority_ms: authority_ms,
        move_peer_observation_ms: peer_ms,
        screenshots: captures
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        contact_sheet: output.to_string_lossy().into_owned(),
    };
    let report_path = output.with_extension("json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("could not encode report: {error}"))?,
    )
    .map_err(|error| format!("could not write acceptance report: {error}"))?;
    println!(
        "two devices accepted: authority={authority_ms:.2} ms, peer={peer_ms:.2} ms\ncontact sheet: {}\nreport: {}",
        output.display(),
        report_path.display()
    );
    Ok(())
}

fn capture(root: &Path) -> Result<PathBuf, String> {
    let response = send(root, FileControlAction::Capture)?;
    response
        .capture_path
        .map(PathBuf::from)
        .ok_or("capture response omitted its path".into())
}

fn wait_until(
    root: &Path,
    predicate: impl Fn(&FileControlObservation) -> bool,
) -> Result<FileControlObservation, String> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let observation = send(
            root,
            FileControlAction::Observe {
                include_join_code: false,
            },
        )?
        .observation;
        if predicate(&observation) {
            return Ok(observation);
        }
        if Instant::now() >= deadline {
            return Err("device observation did not satisfy the acceptance condition".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_descriptor(root: &Path) -> Result<(), String> {
    let deadline = Instant::now() + TIMEOUT;
    while !root.join("instance.json").exists() {
        if Instant::now() >= deadline {
            return Err(format!("{} did not publish its descriptor", root.display()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn spawn_device(
    executable: &Path,
    root: &Path,
    instance_id: &str,
    log_root: &Path,
    authority: &AuthorityEndpoint,
) -> Result<Child, String> {
    let stdout = File::create(log_root.join(format!("{instance_id}.stdout.log")))
        .map_err(|error| format!("could not create device stdout log: {error}"))?;
    let stderr = File::create(log_root.join(format!("{instance_id}.stderr.log")))
        .map_err(|error| format!("could not create device stderr log: {error}"))?;
    let mut command = Command::new(executable);
    command.args([
        "--control-root",
        &root.to_string_lossy(),
        "--instance-id",
        instance_id,
        "--windowless",
    ]);
    command.args(["--server", &authority.uri]);
    command.args(["--database", &authority.database]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("could not launch {}: {error}", executable.display()))
}

fn stop_and_wait(root: &Path, child: &mut Child) {
    if root.join("instance.json").exists() {
        let _ =
            send_file_control_request(root, FileControlAction::Stop, Some(Duration::from_secs(3)));
    }
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn compose_contact_sheet(captures: &[PathBuf; 4], output: &Path) -> Result<(), String> {
    let images = captures
        .iter()
        .map(|path| {
            image::open(path)
                .map(|image| image.to_rgba8())
                .map_err(|error| format!("could not read {}: {error}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let width = images.iter().map(RgbaImage::width).max().unwrap_or(1);
    let height = images.iter().map(RgbaImage::height).max().unwrap_or(1);
    let gutter = 12_u32;
    let mut sheet = RgbaImage::from_pixel(
        width * 2 + gutter * 3,
        height * 2 + gutter * 3,
        Rgba([18, 24, 25, 255]),
    );
    for (index, image) in images.iter().enumerate() {
        let column = u32::try_from(index % 2).unwrap_or_default();
        let row = u32::try_from(index / 2).unwrap_or_default();
        let x = gutter + column * (width + gutter);
        let y = gutter + row * (height + gutter);
        imageops::overlay(&mut sheet, image, i64::from(x), i64::from(y));
    }
    sheet
        .save(output)
        .map_err(|error| format!("could not write {}: {error}", output.display()))
}

fn absolute(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| format!("could not resolve output path: {error}"))
    }
}

fn unix_millis() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system time is unavailable")?
            .as_millis(),
    )
    .map_err(|_| "system time does not fit milliseconds".into())
}

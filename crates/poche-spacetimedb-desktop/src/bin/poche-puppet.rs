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
        Some("select-identity") => {
            let root = PathBuf::from(args.next().ok_or("select-identity requires CONTROL_ROOT")?);
            let label = args.next().ok_or("select-identity requires LABEL")?;
            print_response(send(&root, FileControlAction::SelectIdentity { label })?)
        }
        Some("resume") => {
            let root = PathBuf::from(args.next().ok_or("resume requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ResumeLobby)?)
        }
        Some("return-title") => {
            let root = PathBuf::from(args.next().ok_or("return-title requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ReturnToTitle)?)
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
        Some("menu") => {
            let root = PathBuf::from(args.next().ok_or("menu requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ToggleTableMenu)?)
        }
        Some("options") => {
            let root = PathBuf::from(args.next().ok_or("options requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::OpenOptions)?)
        }
        Some("invert-camera-y") => {
            let root = PathBuf::from(args.next().ok_or("invert-camera-y requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ToggleCameraYInversion)?)
        }
        Some("back") => {
            let root = PathBuf::from(args.next().ok_or("back requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::CloseOptions)?)
        }
        Some("leave") => {
            let root = PathBuf::from(args.next().ok_or("leave requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::ActivateLeave)?)
        }
        Some(command @ ("maximize" | "restore")) => {
            let root = PathBuf::from(
                args.next()
                    .ok_or_else(|| format!("{command} requires CONTROL_ROOT"))?,
            );
            print_response(send(
                &root,
                FileControlAction::SetWindowMaximized {
                    maximized: command == "maximize",
                },
            )?)
        }
        Some(command @ ("minimize" | "unminimize")) => {
            let root = PathBuf::from(
                args.next()
                    .ok_or_else(|| format!("{command} requires CONTROL_ROOT"))?,
            );
            print_response(send(
                &root,
                FileControlAction::SetWindowMinimized {
                    minimized: command == "minimize",
                },
            )?)
        }
        Some("bid") => {
            let root = PathBuf::from(args.next().ok_or("bid requires CONTROL_ROOT")?);
            let tricks = parse(&mut args, "TRICKS")?;
            print_response(send(&root, FileControlAction::Bid { tricks })?)
        }
        Some("play") => {
            let root = PathBuf::from(args.next().ok_or("play requires CONTROL_ROOT")?);
            let card_index = parse(&mut args, "CARD_INDEX")?;
            print_response(send(&root, FileControlAction::PlayOwnCard { card_index })?)
        }
        Some("move") => {
            let root = PathBuf::from(args.next().ok_or("move requires CONTROL_ROOT")?);
            let card_index = parse(&mut args, "CARD_INDEX")?;
            let x = parse(&mut args, "X_MM")?;
            let y = parse(&mut args, "Y_MM")?;
            let z = parse(&mut args, "Z_MM")?;
            let ry = parse(&mut args, "RY_MDEG")?;
            print_response(send(
                &root,
                FileControlAction::MoveOwnCard {
                    card_index,
                    position_mm: [x, y, z],
                    rotation_mdeg: [0, ry, 0],
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
                 poche-puppet set-name ROOT NAME | select-identity ROOT LABEL | resume ROOT\n\
                 poche-puppet return-title ROOT | maximize ROOT | restore ROOT\n\
                 poche-puppet minimize ROOT | unminimize ROOT\n\
                 poche-puppet create ROOT | join ROOT CODE\n\
                 poche-puppet seat ROOT 0|1 | stand ROOT | menu ROOT | options ROOT | back ROOT\n\
                 poche-puppet invert-camera-y ROOT | leave ROOT | bid ROOT TRICKS\n\
                 poche-puppet play ROOT CARD_INDEX\n\
                 poche-puppet move ROOT CARD_INDEX X_MM Y_MM Z_MM RY_MDEG\n\
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
    distinct_private_faces: bool,
    initial_public_revealed_cards: usize,
    shared_face_free_poses: usize,
    moved_card_key: String,
    moved_position_mm: [i32; 3],
    move_authority_ms: f64,
    move_peer_observation_ms: f64,
    completed_actions: u64,
    final_phase: String,
    revealed_cards: usize,
    public_activity_events: usize,
    winning_logical_location: String,
    checks: AcceptanceChecks,
    peer_members_after_leave: usize,
    table_menu_capture: String,
    options_default_capture: String,
    options_toggled_capture: String,
    leave_confirmation_capture: String,
    identity_contact_sheet: String,
    screenshots: Vec<String>,
    contact_sheet: String,
}

#[derive(Serialize)]
struct AcceptanceChecks {
    leave_showed_terminal: bool,
    same_identity_resume: bool,
    presence_survived_sibling_disconnect: bool,
}

struct AcceptanceContext<'a> {
    output: &'a Path,
    run_id: &'a str,
    executable: &'a Path,
    run_root: &'a Path,
    alice_root: &'a Path,
    alice_vault: &'a Path,
    bob_root: &'a Path,
    authority: &'a AuthorityEndpoint,
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
    let alice_vault = run_root.join("alice-identities.json");
    let bob_vault = run_root.join("bob-identities.json");
    let mut alice = spawn_device(
        &executable,
        &alice_root,
        "alice-window",
        &run_root,
        authority,
        &alice_vault,
    )?;
    let mut bob = spawn_device(
        &executable,
        &bob_root,
        "bob-window",
        &run_root,
        authority,
        &bob_vault,
    )?;

    let result = acceptance_inner(AcceptanceContext {
        output,
        run_id: &run_id,
        executable: &executable,
        run_root: &run_root,
        alice_root: &alice_root,
        alice_vault: &alice_vault,
        bob_root: &bob_root,
        authority,
    });
    stop_and_wait(&alice_root, &mut alice);
    stop_and_wait(&bob_root, &mut bob);
    result
}

fn acceptance_inner(context: AcceptanceContext<'_>) -> Result<(), String> {
    let AcceptanceContext {
        output,
        run_id,
        executable,
        run_root,
        alice_root,
        alice_vault,
        bob_root,
        authority,
    } = context;
    wait_for_descriptor(alice_root)?;
    wait_for_descriptor(bob_root)?;
    let identity_gate_capture = capture(alice_root)?;
    let short_run = &run_id[..run_id.len().min(12)];
    let alice_label = format!("Alice {short_run}");
    send(
        alice_root,
        FileControlAction::SetName {
            name: alice_label.clone(),
        },
    )?;
    let title_capture = capture(alice_root)?;
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
    send(
        bob_root,
        FileControlAction::JoinLobby {
            join_code: join_code.clone(),
        },
    )?;
    send(alice_root, FileControlAction::TakeSeat { seat: 0 })?;
    send(bob_root, FileControlAction::TakeSeat { seat: 1 })?;
    let alice_ready = wait_until(alice_root, |observation| {
        observation.own_hand.len() == 1
            && observation.game.is_some()
            && observation.rendered_card_count == observation.card_poses.len()
            && observation.rendered_player_count == 2
    })?;
    let bob_ready = wait_until(bob_root, |observation| {
        observation.own_hand.len() == 1
            && observation.game.is_some()
            && observation.rendered_card_count == observation.card_poses.len()
            && observation.rendered_player_count == 2
    })?;
    if !alice_ready.revealed_cards.is_empty() || !bob_ready.revealed_cards.is_empty() {
        return Err("the deal exposed a card face through the public revealed-card view".into());
    }
    for required in ["room-created", "room-joined", "seat-taken", "deal-started"] {
        if !alice_ready
            .activity
            .iter()
            .any(|event| event.kind == required)
        {
            return Err(format!("initial public activity omitted {required:?}"));
        }
    }
    let distinct_private_faces = alice_ready.own_hand[0].face != bob_ready.own_hand[0].face;
    if !distinct_private_faces {
        return Err("the deterministic deal duplicated a card face".into());
    }
    let initial_shared_poses = bob_ready.card_poses.len();

    let seated_alice = capture(alice_root)?;
    let seated_bob = capture(bob_root)?;
    let moved_position = [60, 40, 500];
    let moved_rotation = [0, 45_000, 0];
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

    let mut turn = bob_after;
    for _ in 0..2 {
        let actor = turn
            .game
            .as_ref()
            .and_then(|game| game.actor_seat)
            .ok_or("bidding projection omitted its actor")?;
        let previous = turn.game.as_ref().map_or(0, |game| game.action_count);
        send(
            root_for_seat(actor, alice_root, bob_root),
            FileControlAction::Bid { tricks: 0 },
        )?;
        turn = wait_until(alice_root, |observation| {
            observation
                .game
                .as_ref()
                .is_some_and(|game| game.action_count > previous)
        })?;
    }
    if turn.game.as_ref().map(|game| game.phase.as_str()) != Some("playing") {
        return Err("two legal bids did not enter the playing phase".into());
    }
    for _ in 0..2 {
        let actor = turn
            .game
            .as_ref()
            .and_then(|game| game.actor_seat)
            .ok_or("playing projection omitted its actor")?;
        send(
            root_for_seat(actor, alice_root, bob_root),
            FileControlAction::PlayOwnCard { card_index: 0 },
        )?;
        let previous = turn.game.as_ref().map_or(0, |game| game.action_count);
        turn = wait_until(alice_root, |observation| {
            observation
                .game
                .as_ref()
                .is_some_and(|game| game.action_count > previous)
        })?;
    }
    let alice_resolved = wait_until(alice_root, trick_is_resolved)?;
    let bob_resolved = wait_until(bob_root, trick_is_resolved)?;
    let same_reveals = alice_resolved
        .revealed_cards
        .iter()
        .all(|alice| bob_resolved.revealed_cards.iter().any(|bob| bob == alice));
    if alice_resolved.game != bob_resolved.game || !same_reveals {
        return Err("the two devices did not converge on one public resolved trick".into());
    }
    if alice_resolved.activity != bob_resolved.activity
        || !["bid", "card-played"].into_iter().all(|required| {
            alice_resolved
                .activity
                .iter()
                .any(|event| event.kind == required)
        })
    {
        return Err("the two devices did not converge on one public activity history".into());
    }
    let resolved_alice = capture(alice_root)?;
    let resolved_bob = capture(bob_root)?;
    let captures = [seated_alice, seated_bob, resolved_alice, resolved_bob];
    compose_contact_sheet(&captures, output)?;

    let mirror_root = run_root.join("alice-mirror");
    let mut mirror = spawn_device(
        executable,
        &mirror_root,
        "alice-mirror-window",
        run_root,
        authority,
        alice_vault,
    )?;
    let mut resume_offer_capture = None;
    let mirror_result: Result<(), String> = (|| {
        wait_for_descriptor(&mirror_root)?;
        let selected = send(
            &mirror_root,
            FileControlAction::SelectIdentity {
                label: alice_label.clone(),
            },
        )?
        .observation;
        if selected.surface != "resume_offer" {
            return Err(
                "a second device for Alice did not receive the explicit resume offer".into(),
            );
        }
        if selected.viewer_identity != alice_resolved.viewer_identity
            || selected.own_seat != alice_resolved.own_seat
            || selected.own_hand != alice_resolved.own_hand
            || selected.members != alice_resolved.members
            || selected.card_poses != alice_resolved.card_poses
            || selected.activity != alice_resolved.activity
        {
            return Err(
                "the resumed Alice device did not hydrate the same identity, seat, hand, roster, cards, and activity before entering the table".into(),
            );
        }
        let recovered_capability = send(
            &mirror_root,
            FileControlAction::Observe {
                include_join_code: true,
            },
        )?
        .observation
        .join_code;
        if recovered_capability.as_deref() != Some(join_code.as_str()) {
            return Err(
                "the resumed Alice device did not recover its sender-scoped lobby code".into(),
            );
        }
        resume_offer_capture = Some(capture(&mirror_root)?);
        send(&mirror_root, FileControlAction::ResumeLobby)?;
        let resumed = wait_until(&mirror_root, |observation| {
            observation.surface == "table"
                && observation.rendered_player_count
                    == observation
                        .members
                        .iter()
                        .filter(|member| member.seat.is_some())
                        .count()
                && observation.rendered_card_count == observation.card_poses.len()
        })?;
        if resumed.members != alice_resolved.members
            || resumed.card_poses != alice_resolved.card_poses
            || resumed.activity != alice_resolved.activity
        {
            return Err(
                "the resumed table did not render its preloaded authoritative model".into(),
            );
        }
        Ok(())
    })();
    stop_and_wait(&mirror_root, &mut mirror);
    mirror_result?;
    let resume_offer_capture =
        resume_offer_capture.ok_or("resume offer capture was not produced")?;
    let identity_contact_sheet = output.with_file_name("identity-flow.png");
    compose_contact_sheet(
        &[
            identity_gate_capture.clone(),
            title_capture.clone(),
            resume_offer_capture.clone(),
        ],
        &identity_contact_sheet,
    )?;
    let bob_after_mirror_disconnect = wait_until(bob_root, |observation| {
        observation.members.iter().any(|member| {
            member.identity == alice_resolved.viewer_identity.clone().unwrap_or_default()
                && member.connected
        })
    })?;
    let presence_survived_sibling_disconnect =
        bob_after_mirror_disconnect.members.iter().any(|member| {
            member.identity == alice_resolved.viewer_identity.clone().unwrap_or_default()
                && member.connected
        });

    send(alice_root, FileControlAction::ToggleTableMenu)?;
    let table_menu_capture = capture(alice_root)?;
    send(alice_root, FileControlAction::OpenOptions)?;
    let options_default_capture = capture(alice_root)?;
    let toggled_options = send(alice_root, FileControlAction::ToggleCameraYInversion)?.observation;
    if !toggled_options.status.contains("Invert camera Y: Off") {
        return Err("camera Y inversion did not toggle away from its inverted default".into());
    }
    let options_toggled_capture = capture(alice_root)?;
    send(alice_root, FileControlAction::CloseOptions)?;
    send(alice_root, FileControlAction::ActivateLeave)?;
    let leave_confirmation_capture = capture(alice_root)?;
    let left = send(alice_root, FileControlAction::ActivateLeave)?.observation;
    let leave_showed_terminal = left.surface == "lobby_ended" && left.room_id.is_none();
    if !leave_showed_terminal {
        return Err("leaving did not show the controlling device its lobby-ended screen".into());
    }
    let bob_after_leave = wait_until(bob_root, |observation| {
        observation.members.len() == 1
            && observation
                .members
                .first()
                .is_some_and(|member| member.is_self)
            && observation.game.is_none()
            && observation.card_poses.is_empty()
            && observation
                .activity
                .last()
                .is_some_and(|event| event.kind == "room-left")
    })?;
    let menu_after_leave = capture(alice_root)?;

    let report = AcceptanceReport {
        schema: "poche-spacetimedb-multi-device-acceptance-v8",
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
        distinct_private_faces,
        initial_public_revealed_cards: bob_ready.revealed_cards.len(),
        shared_face_free_poses: initial_shared_poses,
        moved_card_key: moved_key,
        moved_position_mm: moved_position,
        move_authority_ms: authority_ms,
        move_peer_observation_ms: peer_ms,
        completed_actions: alice_resolved
            .game
            .as_ref()
            .map_or(0, |game| game.action_count),
        final_phase: alice_resolved
            .game
            .as_ref()
            .map_or_else(|| "missing".into(), |game| game.phase.clone()),
        revealed_cards: alice_resolved.revealed_cards.len(),
        public_activity_events: alice_resolved.activity.len(),
        winning_logical_location: alice_resolved
            .card_poses
            .first()
            .map_or_else(|| "missing".into(), |pose| pose.logical_location.clone()),
        checks: AcceptanceChecks {
            leave_showed_terminal,
            same_identity_resume: true,
            presence_survived_sibling_disconnect,
        },
        peer_members_after_leave: bob_after_leave.members.len(),
        table_menu_capture: table_menu_capture.to_string_lossy().into_owned(),
        options_default_capture: options_default_capture.to_string_lossy().into_owned(),
        options_toggled_capture: options_toggled_capture.to_string_lossy().into_owned(),
        leave_confirmation_capture: leave_confirmation_capture.to_string_lossy().into_owned(),
        identity_contact_sheet: identity_contact_sheet.to_string_lossy().into_owned(),
        screenshots: captures
            .iter()
            .chain([
                &identity_gate_capture,
                &title_capture,
                &resume_offer_capture,
            ])
            .chain(std::iter::once(&table_menu_capture))
            .chain(std::iter::once(&options_default_capture))
            .chain(std::iter::once(&options_toggled_capture))
            .chain(std::iter::once(&leave_confirmation_capture))
            .chain(std::iter::once(&menu_after_leave))
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

fn root_for_seat<'a>(seat: u8, alice: &'a Path, bob: &'a Path) -> &'a Path {
    if seat == 0 { alice } else { bob }
}

fn trick_is_resolved(observation: &FileControlObservation) -> bool {
    observation.own_hand.is_empty()
        && observation.revealed_cards.len() == 2
        && observation.card_poses.len() == 2
        && observation
            .card_poses
            .iter()
            .all(|pose| pose.logical_location.starts_with("won:"))
        && observation
            .game
            .as_ref()
            .is_some_and(|game| game.phase == "scoring" && game.action_count == 4)
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
    identity_vault: &Path,
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
        "--identity-vault",
        &identity_vault.to_string_lossy(),
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

fn compose_contact_sheet(captures: &[PathBuf], output: &Path) -> Result<(), String> {
    if captures.is_empty() {
        return Err("a contact sheet requires at least one capture".into());
    }
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
    let rows = u32::try_from(captures.len().div_ceil(2)).unwrap_or(1);
    let mut sheet = RgbaImage::from_pixel(
        width * 2 + gutter * 3,
        height * rows + gutter * (rows + 1),
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

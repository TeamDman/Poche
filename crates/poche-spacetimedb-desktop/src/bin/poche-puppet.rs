// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Disposable two-device acceptance puppet plus an ad-hoc client for a live
//! Poche file-control endpoint.

#![allow(clippy::needless_pass_by_value, clippy::too_many_lines)]

use image::{Rgba, RgbaImage, imageops};
use poche_spacetimedb_desktop::AuthorityEndpoint;
use poche_spacetimedb_desktop::file_control::{
    FileControlAction, FileControlCardPose, FileControlObservation, FileControlResponse,
    FileControlStatus, send_file_control_request,
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
        Some("deal") => {
            let root = PathBuf::from(args.next().ok_or("deal requires CONTROL_ROOT")?);
            print_response(send(&root, FileControlAction::DealNextRound)?)
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
        Some("move-coin") => {
            let root = PathBuf::from(args.next().ok_or("move-coin requires CONTROL_ROOT")?);
            let coin_id = args.next().ok_or("move-coin requires COIN_ID")?;
            let container = args.next().ok_or("move-coin requires jar|lid|bowl")?;
            let x = parse(&mut args, "X_MM")?;
            let y = parse(&mut args, "Y_MM")?;
            let z = parse(&mut args, "Z_MM")?;
            let mode = args.next().ok_or("move-coin requires commit|preview")?;
            if !matches!(mode.as_str(), "commit" | "preview") {
                return Err("move-coin requires commit|preview".into());
            }
            print_response(send(
                &root,
                FileControlAction::MoveCoin {
                    coin_id,
                    container,
                    position_mm: [x, y, z],
                    commit: mode == "commit",
                },
            )?)
        }
        Some("pointer") => {
            let root = PathBuf::from(args.next().ok_or("pointer requires CONTROL_ROOT")?);
            let x = parse(&mut args, "X_LOGICAL_PX")?;
            let y = parse(&mut args, "Y_LOGICAL_PX")?;
            let primary_down = parse_button_state(&mut args)?;
            print_response(send(
                &root,
                FileControlAction::Pointer { x, y, primary_down },
            )?)
        }
        Some("camera-gesture") => {
            let root = PathBuf::from(args.next().ok_or("camera-gesture requires CONTROL_ROOT")?);
            let dx = parse(&mut args, "DX")?;
            let dy = parse(&mut args, "DY")?;
            let button = args
                .next()
                .ok_or("camera-gesture requires middle|right|up")?;
            if !matches!(button.as_str(), "middle" | "right" | "up") {
                return Err("camera-gesture requires middle|right|up".into());
            }
            print_response(send(
                &root,
                FileControlAction::CameraGesture {
                    delta: [dx, dy],
                    middle_down: button == "middle",
                    right_down: button == "right",
                },
            )?)
        }
        Some("key") => {
            let root = PathBuf::from(args.next().ok_or("key requires CONTROL_ROOT")?);
            let key = args.next().ok_or("key requires KEY")?;
            let down = parse_button_state(&mut args)?;
            print_response(send(&root, FileControlAction::Key { key, down })?)
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
                 poche-puppet play ROOT CARD_INDEX | deal ROOT\n\
                 poche-puppet move ROOT CARD_INDEX X_MM Y_MM Z_MM RY_MDEG\n\
                 poche-puppet move-coin ROOT COIN_ID jar|lid|bowl X_MM Y_MM Z_MM commit|preview\n\
                 poche-puppet pointer ROOT X_LOGICAL_PX Y_LOGICAL_PX down|up (windowless only)\n\
                 poche-puppet camera-gesture ROOT DX DY middle|right|up (windowless only)\n\
                 poche-puppet key ROOT Q|E|I|O|Z|Space|W|A|S|D|ArrowUp|ArrowDown|ArrowLeft|ArrowRight|Escape|F3 down|up\n\
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
        .ok_or_else(|| format!("missing required {label}"))?
        .parse()
        .map_err(|error| format!("invalid {label}: {error}"))
}

fn parse_button_state(args: &mut impl Iterator<Item = String>) -> Result<bool, String> {
    match args.next().as_deref() {
        Some("down") => Ok(true),
        Some("up") => Ok(false),
        _ => Err("input requires down or up".into()),
    }
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
    help_capture: String,
    options_default_capture: String,
    options_toggled_capture: String,
    leave_confirmation_capture: String,
    identity_contact_sheet: String,
    screenshots: Vec<String>,
    contact_sheet: String,
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // Independent evidence flags, not mutable application state.
struct AcceptanceChecks {
    finite_coin_inventory_and_denominations: bool,
    jar_lid_transfer_preserved_total: bool,
    real_pointer_quarter_antes: bool,
    partial_ante_did_not_deal: bool,
    wrong_denomination_and_overpayment_rejected: bool,
    missed_bid_dime_paid_and_conserved: bool,
    scoresheet_camera_pan_projection_and_click_restore: bool,
    wrong_dealer_rejected_without_mutation: bool,
    real_deck_click_dealt_second_round: bool,
    second_round_scored_and_rotated_dealer: bool,
    round_history_scores_and_money_conserved: bool,
    returning_coin_regrabbed_through_real_pointer: bool,
    real_pointer_drag_and_q_rotation: bool,
    real_pointer_winner_repositioned_opponents_won_card: bool,
    hand_world_hand_mapping: bool,
    leave_showed_terminal: bool,
    same_identity_resume: bool,
    presence_survived_sibling_disconnect: bool,
    contextual_money_hover_precedence: bool,
    marquee_coin_count_without_transfers: bool,
    private_hand_size_popup_preserves_camera_and_authority: bool,
    local_pickup_drop_sound_once_without_peer_echo: bool,
    own_seat_and_door_are_real_pointer_actions: bool,
    help_and_options_are_separate_menu_pages: bool,
}

struct AcceptanceContext<'a> {
    output: &'a Path,
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
    let alice_label = "Alice".to_string();
    send(
        alice_root,
        FileControlAction::SetName {
            name: alice_label.clone(),
        },
    )?;
    let title_capture = capture(alice_root)?;
    send(bob_root, FileControlAction::SetName { name: "Bob".into() })?;
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
    click(alice_root, 348., 548.)?; // Visible first stool in the fixed spectator viewport.
    wait_until(alice_root, |observation| observation.own_seat == Some(0))?;
    send(bob_root, FileControlAction::TakeSeat { seat: 1 })?;
    wait_until(bob_root, |observation| observation.own_seat == Some(1))?;
    let money_captures = verify_manual_antes(alice_root, bob_root)?;
    compose_contact_sheet(&money_captures, &output.with_file_name("manual-antes.png"))?;
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
    let contextual_captures = verify_contextual_inspection(alice_root)?;
    compose_contact_sheet(
        &contextual_captures,
        &output.with_file_name("contextual-inspection.png"),
    )?;
    let interaction_captures = verify_hand_input(alice_root, bob_root)?;
    compose_contact_sheet(
        &interaction_captures,
        &output.with_file_name("hand-interaction.png"),
    )?;
    let sheet_captures = verify_sheet_inspection(alice_root)?;
    compose_contact_sheet(
        &sheet_captures,
        &output.with_file_name("scoresheet-inspection.png"),
    )?;
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
        let actor_root = root_for_seat(actor, alice_root, bob_root);
        click(actor_root, 60., 32.)?;
        capture(actor_root)?;
        click(actor_root, 100., 115.)?;
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
    wait_until(bob_root, trick_is_resolved)?;
    let won_card_captures = verify_won_card_input(alice_root, bob_root, &alice_resolved)?;
    compose_contact_sheet(
        &won_card_captures,
        &output.with_file_name("won-card-interaction.png"),
    )?;
    let penalty_captures = verify_missed_bid_payment(alice_root, bob_root, &alice_resolved)?;
    compose_contact_sheet(
        &penalty_captures,
        &output.with_file_name("missed-bid-payment.png"),
    )?;
    let alice_resolved = send(
        alice_root,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation;
    let bob_resolved = wait_until(bob_root, |o| {
        o.game == alice_resolved.game && o.activity == alice_resolved.activity
    })?;
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

    let (continuity_captures, alice_resolved) =
        verify_round_continuity(alice_root, bob_root, &alice_resolved)?;
    compose_contact_sheet(
        &continuity_captures,
        &output.with_file_name("round-continuity.png"),
    )?;

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
    let main_menu = observe(alice_root)?;
    let [help_x, help_y] = main_menu
        .contextual
        .help_button_screen
        .ok_or("Help has no visible button in the table menu")?;
    click(alice_root, help_x, help_y)?;
    wait_until(alice_root, |o| o.contextual.escape_menu_page == "help")?;
    let help_capture = capture(alice_root)?;
    send(
        alice_root,
        FileControlAction::Key {
            key: "Escape".into(),
            down: true,
        },
    )?;
    send(
        alice_root,
        FileControlAction::Key {
            key: "Escape".into(),
            down: false,
        },
    )?;
    let main_menu = wait_until(alice_root, |o| o.contextual.escape_menu_page == "main")?;
    let [options_x, options_y] = main_menu
        .contextual
        .options_button_screen
        .ok_or("Options has no visible button separate from Help")?;
    click(alice_root, options_x, options_y)?;
    wait_until(alice_root, |o| o.contextual.escape_menu_page == "options")?;
    let options_default_capture = capture(alice_root)?;
    let toggled_options = send(alice_root, FileControlAction::ToggleCameraYInversion)?.observation;
    if !toggled_options.status.contains("Invert camera Y: Off") {
        return Err("camera Y inversion did not toggle away from its inverted default".into());
    }
    let options_toggled_capture = capture(alice_root)?;
    send(alice_root, FileControlAction::CloseOptions)?;
    send(alice_root, FileControlAction::ToggleTableMenu)?;
    let seated = observe(alice_root)?;
    let [seat_x, seat_y] = seated
        .contextual
        .own_seat_screen
        .ok_or("own stool was not projected for the stand-up action")?;
    click(alice_root, seat_x, seat_y)?;
    wait_until(alice_root, |o| o.own_seat.is_none() && o.room_id.is_some())?;
    // Standing changes the camera home. Let that ordinary transition settle
    // before sampling the door's current projected target.
    std::thread::sleep(Duration::from_millis(600));
    let standing = observe(alice_root)?;
    let [door_x, door_y] = standing
        .contextual
        .door_screen
        .ok_or("the exit door was not visible from the standing camera")?;
    click(alice_root, door_x, door_y)?;
    let armed = observe(alice_root)?;
    if armed.room_id != standing.room_id || !armed.status.contains("again") {
        return Err("the first door click did not arm a non-destructive leave confirmation".into());
    }
    let leave_confirmation_capture = capture(alice_root)?;
    click(alice_root, door_x, door_y)?;
    let left = wait_until(alice_root, |o| {
        o.surface == "lobby_ended" && o.room_id.is_none()
    })?;
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
        schema: "poche-spacetimedb-multi-device-acceptance-v13",
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
            finite_coin_inventory_and_denominations: true,
            jar_lid_transfer_preserved_total: true,
            real_pointer_quarter_antes: true,
            partial_ante_did_not_deal: true,
            wrong_denomination_and_overpayment_rejected: true,
            missed_bid_dime_paid_and_conserved: true,
            scoresheet_camera_pan_projection_and_click_restore: true,
            wrong_dealer_rejected_without_mutation: true,
            real_deck_click_dealt_second_round: true,
            second_round_scored_and_rotated_dealer: true,
            round_history_scores_and_money_conserved: true,
            returning_coin_regrabbed_through_real_pointer: true,
            real_pointer_drag_and_q_rotation: true,
            real_pointer_winner_repositioned_opponents_won_card: true,
            hand_world_hand_mapping: true,
            leave_showed_terminal,
            same_identity_resume: true,
            presence_survived_sibling_disconnect,
            contextual_money_hover_precedence: true,
            marquee_coin_count_without_transfers: true,
            private_hand_size_popup_preserves_camera_and_authority: true,
            local_pickup_drop_sound_once_without_peer_echo: true,
            own_seat_and_door_are_real_pointer_actions: true,
            help_and_options_are_separate_menu_pages: true,
        },
        peer_members_after_leave: bob_after_leave.members.len(),
        table_menu_capture: table_menu_capture.to_string_lossy().into_owned(),
        help_capture: help_capture.to_string_lossy().into_owned(),
        options_default_capture: options_default_capture.to_string_lossy().into_owned(),
        options_toggled_capture: options_toggled_capture.to_string_lossy().into_owned(),
        leave_confirmation_capture: leave_confirmation_capture.to_string_lossy().into_owned(),
        identity_contact_sheet: identity_contact_sheet.to_string_lossy().into_owned(),
        screenshots: captures
            .iter()
            .chain(interaction_captures.iter())
            .chain(contextual_captures.iter())
            .chain(sheet_captures.iter())
            .chain(money_captures.iter())
            .chain(penalty_captures.iter())
            .chain(won_card_captures.iter())
            .chain(continuity_captures.iter())
            .chain([
                &identity_gate_capture,
                &title_capture,
                &resume_offer_capture,
            ])
            .chain(std::iter::once(&table_menu_capture))
            .chain(std::iter::once(&help_capture))
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
            .is_some_and(|game| game.phase == "scoring" && game.round_index == 0)
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

/// Exercise the same input systems as a person, not MoveOwnCard/reducer calls.
fn verify_hand_input(owner: &Path, peer: &Path) -> Result<Vec<PathBuf>, String> {
    let initial = send(
        owner,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation;
    let peer_before = observe(peer)?;
    let key = initial
        .own_hand
        .first()
        .ok_or("input test requires an owned card")?
        .card_key
        .clone();
    let source = initial
        .card_poses
        .iter()
        .find(|pose| pose.card_key == key)
        .ok_or("missing input-test pose")?;
    let height = source.position_mm[1];
    let angle = (source.rotation_mdeg[1] + 45_000).rem_euclid(360_000);
    let pointer =
        |root, x, y, primary_down| send(root, FileControlAction::Pointer { x, y, primary_down });
    pointer(owner, 590., 732., false)?;
    let hover = capture(owner)?;
    pointer(owner, 590., 732., true)?;
    wait_until(peer, |observation| {
        observation
            .card_poses
            .iter()
            .any(|pose| pose.card_key == key && pose.position_mm[1] > height)
    })?;
    send(
        owner,
        FileControlAction::Key {
            key: "Q".into(),
            down: true,
        },
    )?;
    send(
        owner,
        FileControlAction::Key {
            key: "Q".into(),
            down: false,
        },
    )?;
    wait_until(peer, |observation| {
        observation
            .card_poses
            .iter()
            .any(|pose| pose.card_key == key && pose.rotation_mdeg[1] == angle)
    })?;
    // Capture while held also provides time for late echoes to arrive.
    let rotated = capture(owner)?;
    pointer(owner, 600., 350., true)?;
    wait_until(peer, |observation| {
        observation
            .card_poses
            .iter()
            .any(|pose| pose.card_key == key && pose.position_mm[2].abs() < 350)
    })?;
    let world = capture(owner)?;
    pointer(owner, 590., 732., true)?;
    pointer(owner, 590., 732., false)?;
    let released = wait_until(peer, |observation| {
        observation.card_poses.iter().any(|pose| {
            pose.card_key == key
                && pose.position_mm[1] == height
                && pose.position_mm[2] > 400
                && pose.rotation_mdeg[1] == angle
        })
    })?;
    if !released.revealed_cards.is_empty()
        || !released
            .card_poses
            .iter()
            .any(|pose| pose.card_key == key && pose.logical_location == source.logical_location)
    {
        return Err("physical hand/world drag changed logical ownership or exposed a face".into());
    }
    wait_until(owner, |observation| {
        observation.held_card_key.is_none() && observation.visible_hand_copies == 1
    })?;
    let returned = capture(owner)?;
    send(
        owner,
        FileControlAction::Key {
            key: "Z".into(),
            down: true,
        },
    )?;
    send(
        owner,
        FileControlAction::Key {
            key: "ArrowDown".into(),
            down: true,
        },
    )?;
    std::thread::sleep(Duration::from_millis(1300));
    send(
        owner,
        FileControlAction::Key {
            key: "ArrowDown".into(),
            down: false,
        },
    )?;
    let low_angle = capture(owner)?;
    send(
        owner,
        FileControlAction::Key {
            key: "Z".into(),
            down: false,
        },
    )?;
    send(
        owner,
        FileControlAction::Key {
            key: "Space".into(),
            down: true,
        },
    )?;
    send(
        owner,
        FileControlAction::Key {
            key: "Space".into(),
            down: false,
        },
    )?;
    std::thread::sleep(Duration::from_millis(600));
    let finished = observe(owner)?;
    let peer_finished = observe(peer)?;
    if finished.contextual.sound_pickup_count != initial.contextual.sound_pickup_count + 1
        || finished.contextual.sound_release_count != initial.contextual.sound_release_count + 1
        || finished.contextual.sound_audible_count != 0
        || peer_finished.contextual.sound_pickup_count != peer_before.contextual.sound_pickup_count
        || peer_finished.contextual.sound_release_count
            != peer_before.contextual.sound_release_count
        || peer_finished.contextual.sound_audible_count != 0
    {
        return Err(
            "card gesture feedback repeated, echoed on the observer, or made windowless audio"
                .into(),
        );
    }
    Ok(vec![hover, rotated, world, returned, low_angle])
}

fn observe(root: &Path) -> Result<FileControlObservation, String> {
    Ok(send(
        root,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation)
}

fn verify_contextual_inspection(root: &Path) -> Result<Vec<PathBuf>, String> {
    let before = observe(root)?;
    let camera = before
        .camera
        .as_ref()
        .ok_or("contextual inspection has no camera")?;
    // The existing input test uses the same visible private card. This is real
    // RMB input, not a semantic 'open popup' shortcut.
    send(
        root,
        FileControlAction::Pointer {
            x: 590.,
            y: 732.,
            primary_down: false,
        },
    )?;
    send(
        root,
        FileControlAction::CameraGesture {
            delta: [0., 0.],
            middle_down: false,
            right_down: true,
        },
    )?;
    let opened = wait_until(root, |o| o.contextual.hand_popup)?;
    let [min_x, min_y, max_x, max_y] = opened
        .contextual
        .hand_slider
        .ok_or("hand size popup did not expose its actual slider rectangle")?;
    let slider_y = (min_y + max_y) * 0.5;
    let scale_x = |scale: f32| min_x + (scale - 0.75) / (2.5 - 0.75) * (max_x - min_x);
    click(root, scale_x(2.0), slider_y)?;
    wait_until(root, |o| (o.contextual.hand_scale - 2.0).abs() < 0.01)?;
    let enlarged = capture(root)?;
    click(root, scale_x(1.0), slider_y)?;
    wait_until(root, |o| (o.contextual.hand_scale - 1.0).abs() < 0.01)?;
    // Dismiss while RMB remains held, then move it. It must not become orbit
    // halfway through the gesture that began as a hand context-menu request.
    send(
        root,
        FileControlAction::Key {
            key: "Escape".into(),
            down: true,
        },
    )?;
    send(
        root,
        FileControlAction::Key {
            key: "Escape".into(),
            down: false,
        },
    )?;
    send(
        root,
        FileControlAction::CameraGesture {
            delta: [40., 20.],
            middle_down: false,
            right_down: true,
        },
    )?;
    send(
        root,
        FileControlAction::CameraGesture {
            delta: [0., 0.],
            middle_down: false,
            right_down: false,
        },
    )?;
    let resized = observe(root)?;
    let after_camera = resized
        .camera
        .as_ref()
        .ok_or("camera missing after hand resize")?;
    if resized.contextual.hand_popup
        || resized.contextual.escape_menu_page != "closed"
        || (after_camera.yaw - camera.yaw).abs() > 0.001
        || (after_camera.pitch - camera.pitch).abs() > 0.001
        || (after_camera.distance - camera.distance).abs() > 0.001
        || resized.card_poses != before.card_poses
        || resized.coins != before.coins
        || resized.held_card_key.is_some()
        || resized.held_coin.is_some()
    {
        return Err(
            "hand popup changed camera/shared pieces, opened Esc, or started a drag".into(),
        );
    }

    let container_hover = verify_container_hover(root, &resized)?;
    let target = resized
        .money_pick_targets
        .iter()
        .filter(|target| target.container == "lid")
        .min_by(|a, b| a.screen[1].total_cmp(&b.screen[1]))
        .ok_or("hover precedence needs a visible coin on the player's lid")?;
    let coin = resized
        .coins
        .iter()
        .find(|c| c.is_own && c.coin_id == target.coin_id)
        .ok_or("hover coin is absent from the inventory")?;
    send(
        root,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: false,
        },
    )?;
    wait_until(root, |o| {
        o.contextual.hovered_coin_key.as_deref() == Some(coin.coin_key.as_str())
            && o.contextual.money_hover.is_none()
    })?;
    let coin_hover = capture(root)?;

    // Screen-space centroid selection is independently evaluated here using
    // the diagnostics' projected public centres, including overlapping coins.
    // The corners are empty floor, not an object on the shared table.
    let start = [8., 170.];
    let end = [1172., 675.];
    let empty = send(
        root,
        FileControlAction::Pointer {
            x: start[0],
            y: start[1],
            primary_down: false,
        },
    )?
    .observation;
    if empty.contextual.hovered_coin_key.is_some() || empty.contextual.world_hover.is_some() {
        return Err("marquee starting point is not empty space".into());
    }
    let expected: std::collections::HashSet<_> = empty
        .contextual
        .projected_coins
        .iter()
        .filter(|(_, p)| p[0] >= start[0] && p[0] <= end[0] && p[1] >= start[1] && p[1] <= end[1])
        .map(|(key, _)| key.clone())
        .collect();
    if expected.is_empty() {
        return Err("marquee test rectangle contains no projected coin centres".into());
    }
    let expected_cents: u32 = empty
        .coins
        .iter()
        .filter(|coin| expected.contains(&coin.coin_key))
        .map(|coin| u32::from(coin.denomination_cents))
        .sum();
    send(
        root,
        FileControlAction::Pointer {
            x: start[0],
            y: start[1],
            primary_down: true,
        },
    )?;
    send(
        root,
        FileControlAction::Pointer {
            x: end[0],
            y: end[1],
            primary_down: true,
        },
    )?;
    let selected = wait_until(root, |o| {
        o.contextual.selecting && o.contextual.selected_cents == expected_cents
    })?;
    let selected_keys: std::collections::HashSet<_> = selected
        .contextual
        .selected_coin_keys
        .iter()
        .cloned()
        .collect();
    if selected_keys != expected || selected.held_card_key.is_some() || selected.held_coin.is_some()
    {
        return Err(
            "marquee preview differs from projected-centre selection or picked up an object".into(),
        );
    }
    let preview = capture(root)?;
    send(
        root,
        FileControlAction::Pointer {
            x: end[0],
            y: end[1],
            primary_down: false,
        },
    )?;
    let retained = observe(root)?;
    if retained.contextual.selecting
        || retained.contextual.selected_coin_keys != selected.contextual.selected_coin_keys
        || retained.contextual.selected_cents != expected_cents
        || retained.coins != before.coins
        || retained.card_poses != before.card_poses
    {
        return Err(
            "selection did not persist on release, or changed authoritative money/cards".into(),
        );
    }
    let selected_capture = capture(root)?;
    click(root, start[0], start[1])?;
    wait_until(root, |o| {
        o.contextual.selected_coin_keys.is_empty()
            && o.contextual.selected_card_keys.is_empty()
            && o.contextual.selected_cents == 0
    })?;
    Ok(vec![
        enlarged,
        container_hover,
        coin_hover,
        preview,
        selected_capture,
    ])
}

fn verify_container_hover(
    root: &Path,
    snapshot: &FileControlObservation,
) -> Result<PathBuf, String> {
    let member = snapshot
        .members
        .iter()
        .find(|m| m.is_self)
        .ok_or("container hover needs the local player's name")?;
    // At most twenty ordinary pointer samples. The projected base centre may
    // land on a coin; sample exposed glass/rim beside it, never bypass picking.
    for (container, screen) in [
        ("jar", snapshot.money_jar_screen),
        ("lid", snapshot.money_lid_screen),
    ] {
        let Some([center_x, center_y]) = screen else {
            continue;
        };
        let coins: Vec<_> = snapshot
            .coins
            .iter()
            .filter(|c| c.is_own && c.container == container)
            .collect();
        let cents: u32 = coins.iter().map(|c| u32::from(c.denomination_cents)).sum();
        let expected = format!(
            "{}'s {container} · {} {} · ${}.{:02}",
            member.display_name,
            coins.len(),
            if coins.len() == 1 { "coin" } else { "coins" },
            cents / 100,
            cents % 100
        );
        for [dx, dy] in [
            [-18., -12.],
            [18., -12.],
            [-24., -25.],
            [24., -25.],
            [-12., -40.],
            [12., -40.],
            [0., -55.],
            [-20., 0.],
            [20., 0.],
            [0., 8.],
        ] {
            let observation = send(
                root,
                FileControlAction::Pointer {
                    x: center_x + dx,
                    y: center_y + dy,
                    primary_down: false,
                },
            )?
            .observation;
            if observation.contextual.money_hover.as_deref() == Some(expected.as_str()) {
                if observation.contextual.hovered_coin_key.is_some() {
                    return Err(
                        "container total is visible while its coin has hover priority".into(),
                    );
                }
                return capture(root);
            }
        }
    }
    Err("no exposed jar/lid hover showed the exact public coin count and dollar total".into())
}

fn bowl_cents(observation: &FileControlObservation) -> u32 {
    observation
        .coins
        .iter()
        .filter(|coin| coin.container == "bowl")
        .map(|coin| u32::from(coin.denomination_cents))
        .sum()
}

fn verify_initial_inventory(observation: &FileControlObservation) -> Result<(), String> {
    let own: Vec<_> = observation
        .coins
        .iter()
        .filter(|coin| coin.is_own)
        .collect();
    let quarters = own
        .iter()
        .filter(|coin| coin.denomination_cents == 25)
        .count();
    let dimes = own
        .iter()
        .filter(|coin| coin.denomination_cents == 10)
        .count();
    let total: u32 = own
        .iter()
        .map(|coin| u32::from(coin.denomination_cents))
        .sum();
    let lid: u32 = own
        .iter()
        .filter(|coin| coin.container == "lid")
        .map(|coin| u32::from(coin.denomination_cents))
        .sum();
    if quarters != 100 || dimes != 100 || own.len() != 200 || total != 3_500 || lid != 200 {
        return Err(format!(
            "unexpected initial inventory: {quarters} quarters, {dimes} dimes, total {total}c, lid {lid}c"
        ));
    }
    if observation.game.is_some()
        || !observation.own_hand.is_empty()
        || bowl_cents(observation) != 0
    {
        return Err("seating started a deal before the two manual antes".into());
    }
    Ok(())
}

fn verify_manual_antes(alice: &Path, bob: &Path) -> Result<Vec<PathBuf>, String> {
    let alice_initial = wait_until(alice, |o| {
        o.coins.len() == 400 && !o.money_pick_targets.is_empty()
    })?;
    let bob_initial = wait_until(bob, |o| {
        o.coins.len() == 400 && !o.money_pick_targets.is_empty()
    })?;
    verify_initial_inventory(&alice_initial)?;
    verify_initial_inventory(&bob_initial)?;
    let unpaid = capture(alice)?;

    // Explicit ad-hoc coin commands remain useful for diagnostics; the actual
    // ante below deliberately uses the same projected pointer path as a human.
    let jar_coin = alice_initial
        .coins
        .iter()
        .find(|coin| coin.is_own && coin.container == "jar" && coin.denomination_cents == 10)
        .ok_or("initial jar has no dime")?;
    send(
        alice,
        FileControlAction::MoveCoin {
            coin_id: jar_coin.coin_id.clone(),
            container: "lid".into(),
            position_mm: jar_coin.position_mm,
            commit: true,
        },
    )?;
    let peer_lid = wait_until(bob, |o| {
        o.coins
            .iter()
            .any(|coin| coin.coin_key == jar_coin.coin_key && coin.container == "lid")
    })?;
    let before_total: u32 = bob_initial
        .coins
        .iter()
        .map(|c| u32::from(c.denomination_cents))
        .sum();
    if peer_lid
        .coins
        .iter()
        .map(|c| u32::from(c.denomination_cents))
        .sum::<u32>()
        != before_total
    {
        return Err("jar-to-lid transfer changed the conserved coin value".into());
    }
    send(
        alice,
        FileControlAction::MoveCoin {
            coin_id: jar_coin.coin_id.clone(),
            container: "jar".into(),
            position_mm: jar_coin.position_mm,
            commit: true,
        },
    )?;
    wait_until(bob, |o| {
        o.coins
            .iter()
            .any(|coin| coin.coin_key == jar_coin.coin_key && coin.container == "jar")
    })?;

    let regrab_captures = verify_coin_return_regrab(alice, bob)?;

    drag_quarter_to_bowl(alice)?;
    let alice_partial = wait_until(alice, |o| bowl_cents(o) == 25)?;
    let bob_partial = wait_until(bob, |o| bowl_cents(o) == 25)?;
    for observation in [&alice_partial, &bob_partial] {
        if observation.game.is_some()
            || !observation.own_hand.is_empty()
            || !observation.card_poses.is_empty()
        {
            return Err("a single player's ante incorrectly started the deal".into());
        }
    }
    let partial = capture(bob)?;
    let bowl_pose = alice_partial
        .coins
        .iter()
        .find(|coin| coin.container == "bowl")
        .ok_or("accepted ante has no bowl coin")?
        .position_mm;
    // Alice already paid; neither an extra quarter nor a dime may enter the
    // bowl. Rejections must be surfaced promptly, not mistaken for timeouts.
    for denomination in [10, 25] {
        let extra = alice_partial
            .coins
            .iter()
            .find(|coin| {
                coin.is_own && coin.container == "lid" && coin.denomination_cents == denomination
            })
            .ok_or("missing coin for rejected payment probe")?;
        let response = send_file_control_request(
            alice,
            FileControlAction::MoveCoin {
                coin_id: extra.coin_id.clone(),
                container: "bowl".into(),
                position_mm: bowl_pose,
                commit: true,
            },
            Some(TIMEOUT),
        )?;
        if response.status != FileControlStatus::Rejected
            || !response
                .error
                .as_deref()
                .is_some_and(|error| error.contains("pay exactly"))
            || bowl_cents(&response.observation) != 25
        {
            return Err(format!(
                "{denomination}c overpayment was not correctly rejected: {:?}",
                response.error
            ));
        }
        if response
            .observation
            .coins
            .iter()
            .find(|coin| coin.coin_key == extra.coin_key)
            != Some(extra)
        {
            return Err("rejected payment modified the coin's accepted state".into());
        }
    }
    drag_quarter_to_bowl(bob)?;
    for root in [alice, bob] {
        let dealt = wait_until(root, |o| {
            bowl_cents(o) == 50 && o.game.is_some() && o.own_hand.len() == 1
        })?;
        if dealt.game.as_ref().map(|game| game.pot_cents) != Some(50) {
            return Err("the rule-engine pot disagrees with the physical bowl".into());
        }
    }
    let paid = capture(alice)?;
    let mut captures = vec![unpaid, partial, paid];
    captures.extend(regrab_captures);
    Ok(captures)
}

fn verify_coin_return_regrab(owner: &Path, peer: &Path) -> Result<Vec<PathBuf>, String> {
    let ready = wait_until(owner, |o| {
        o.money_pick_targets
            .iter()
            .any(|target| target.container == "lid" && target.denomination_cents == 25)
    })?;
    let target = ready
        .money_pick_targets
        .iter()
        .find(|target| target.container == "lid" && target.denomination_cents == 25)
        .ok_or("no visible lid quarter for return interruption")?;
    let coin = ready
        .coins
        .iter()
        .find(|coin| coin.is_own && coin.coin_id == target.coin_id)
        .ok_or("visible quarter missing from money snapshot")?;
    send(
        owner,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: false,
        },
    )?;
    let hover = capture(owner)?;
    let grabbed = send(
        owner,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: true,
        },
    )?;
    if grabbed
        .observation
        .held_coin
        .as_ref()
        .map(|held| &held.coin_key)
        != Some(&coin.coin_key)
    {
        return Err("real pointer did not grab the visible quarter".into());
    }
    let table = [0, 20, 330];
    send(
        owner,
        FileControlAction::PointerAtWorld {
            position_mm: table,
            primary_down: true,
        },
    )?;
    send(
        owner,
        FileControlAction::PointerAtWorld {
            position_mm: table,
            primary_down: false,
        },
    )?;
    // Point at its currently drawn center, not a cached 10 Hz pick coordinate or
    // its destination. This still goes through the normal next-frame ray test.
    let regrabbed = send(
        owner,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: true,
        },
    )?;
    let held = regrabbed
        .observation
        .held_coin
        .ok_or("returning coin was not pickable before reaching the lid")?;
    let separation = (f64::from(held.position_m[0]) - f64::from(coin.position_mm[0]) / 1_000.)
        .abs()
        + (f64::from(held.position_m[2]) - f64::from(coin.position_mm[2]) / 1_000.).abs();
    if held.coin_key != coin.coin_key || separation < 0.01 {
        return Err(
            "coin regrab did not interrupt the still-moving return away from its lid".into(),
        );
    }
    send(
        owner,
        FileControlAction::PointerAtWorld {
            position_mm: [80, 20, 330],
            primary_down: true,
        },
    )?;
    wait_until(peer, |o| {
        o.coins.iter().any(|remote| {
            remote.coin_key == coin.coin_key
                && remote.sequence > held.sequence
                && remote.position_mm[0] > 20
                && remote.container == "lid"
        })
    })?;
    let stable = send(
        owner,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation;
    if !stable
        .held_coin
        .as_ref()
        .is_some_and(|held| held.coin_key == coin.coin_key && held.position_m[0] > 0.02)
    {
        return Err("old return acknowledgement stole the quarter from the new drag".into());
    }
    let interrupted = capture(owner)?;
    for primary_down in [true, false] {
        send(
            owner,
            FileControlAction::PointerAtWorld {
                position_mm: [-220, 20, 430],
                primary_down,
            },
        )?;
    }
    wait_until(owner, |o| {
        o.held_coin.is_none()
            && o.coins.iter().any(|current| {
                current.coin_key == coin.coin_key
                    && current.sequence > held.sequence
                    && current.container == "lid"
            })
    })?;
    Ok(vec![hover, interrupted])
}

fn drag_quarter_to_bowl(root: &Path) -> Result<(), String> {
    drag_coin_to_bowl(root, 25)
}

/// Only ownership relative to the viewer differs between otherwise identical
/// card projections. Keep every canonical field in the equality check.
fn same_canonical_card(left: &FileControlCardPose, right: &FileControlCardPose) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.is_own = false;
    right.is_own = false;
    left == right
}

/// A revealed opponent-owned card must be grabbable by its trick winner through
/// the same ray picking and drag systems as a human, not just a reducer call.
fn verify_won_card_input(
    alice: &Path,
    bob: &Path,
    resolved: &FileControlObservation,
) -> Result<Vec<PathBuf>, String> {
    let card = resolved
        .card_poses
        .iter()
        .find(|card| {
            card.logical_location.starts_with("won:")
                && card.logical_location != format!("won:{}", card.owner_seat)
        })
        .ok_or("won-card input test requires a captured opponent's card")?;
    let winner = card
        .logical_location
        .strip_prefix("won:")
        .ok_or("missing won-card seat")?
        .parse::<u8>()
        .map_err(|error| format!("invalid won-card seat: {error}"))?;
    let owner = root_for_seat(winner, alice, bob);
    let peer = root_for_seat(1 - winner, alice, bob);
    wait_until(owner, |o| {
        o.card_poses
            .iter()
            .any(|current| same_canonical_card(current, card))
    })?;
    // Give the normal 18/s interpolation time to settle at this stationary
    // authority pose. PointerAtWorld projects through the live table camera;
    // no fixed screen coordinates or alternate card-mutation API are involved.
    std::thread::sleep(Duration::from_millis(500));
    let before = capture(owner)?;
    let mut source = card.position_mm;
    // Pick the exposed outer strip of the fan rather than its overlapped centre.
    let other = resolved
        .card_poses
        .iter()
        .find(|other| {
            other.card_key != card.card_key && other.logical_location == card.logical_location
        })
        .ok_or("won-card input test requires the completed two-card trick")?;
    source[0] += if source[0] < other.position_mm[0] {
        -20
    } else {
        20
    };
    source[1] += 2;
    for primary_down in [false, true] {
        let response = send(
            owner,
            FileControlAction::PointerAtWorld {
                position_mm: source,
                primary_down,
            },
        )?;
        if primary_down && response.observation.held_card_key.as_ref() != Some(&card.card_key) {
            return Err(format!(
                "winner pointer did not grab captured card {}; held {:?}",
                card.card_key, response.observation.held_card_key
            ));
        }
    }
    let mut destination = source;
    destination[0] += 120;
    for primary_down in [true, false] {
        send(
            owner,
            FileControlAction::PointerAtWorld {
                position_mm: destination,
                primary_down,
            },
        )?;
    }
    let moved = wait_until(peer, |o| {
        o.card_poses.iter().any(|current| {
            current.card_key == card.card_key
                && current.sequence > card.sequence
                && current.position_mm[0] > card.position_mm[0] + 60
                && current.position_mm[1] == card.position_mm[1]
                && current.logical_location == card.logical_location
        })
    })?;
    if moved.game != resolved.game
        || moved.revealed_cards != resolved.revealed_cards
        || moved.rounds != resolved.rounds
    {
        return Err("repositioning a won card changed the game, score, or revealed faces".into());
    }
    wait_until(owner, |o| {
        o.held_card_key.is_none()
            && o.card_poses.len() == moved.card_poses.len()
            && o.card_poses
                .iter()
                .zip(&moved.card_poses)
                .all(|(local, remote)| same_canonical_card(local, remote))
    })?;
    Ok(vec![before, capture(owner)?, capture(peer)?])
}

fn verify_missed_bid_payment(
    alice: &Path,
    bob: &Path,
    resolved: &FileControlObservation,
) -> Result<Vec<PathBuf>, String> {
    let game = resolved.game.as_ref().ok_or("missing resolved game")?;
    let missed: Vec<_> = (0..2)
        .filter(|seat| game.bids[*seat] != Some(game.tricks_won[*seat]))
        .collect();
    if missed.len() != 1 || bowl_cents(resolved) != 50 {
        return Err("two zero bids must leave exactly one missed-bid dime unpaid".into());
    }
    let payer = root_for_seat(
        u8::try_from(missed[0]).map_err(|_| "invalid missed-bid seat")?,
        alice,
        bob,
    );
    let before = capture(payer)?;
    drag_coin_to_bowl(payer, 10)?;
    for root in [alice, bob] {
        let paid = wait_until(root, |o| {
            bowl_cents(o) == 60
                && o.rounds.len() == 1
                && o.payment_due_cents == [0, 0]
                && o.game
                    .as_ref()
                    .is_some_and(|game| game.phase == "awaiting-deal" && game.pot_cents == 60)
        })?;
        if paid.coins.len() != 400
            || paid
                .coins
                .iter()
                .map(|coin| u32::from(coin.denomination_cents))
                .sum::<u32>()
                != 7_000
        {
            return Err("missed-bid payment changed the conserved inventory".into());
        }
    }
    Ok(vec![before, capture(payer)?])
}

/// Continue the real room beyond the previously terminal first-round slice.
fn verify_round_continuity(
    alice: &Path,
    bob: &Path,
    first: &FileControlObservation,
) -> Result<(Vec<PathBuf>, FileControlObservation), String> {
    let game = first.game.as_ref().ok_or("missing paid first round")?;
    if game.phase != "awaiting-deal"
        || game.dealer_seat != Some(1)
        || game.round_index != 1
        || first.rounds.len() != 1
        || first.payment_due_cents != [0, 0]
    {
        return Err(
            "paid first round did not rotate to Bob with one complete scoresheet row".into(),
        );
    }
    let first_row = first.rounds[0].clone();
    if first_row.totals != game.scores || bowl_cents(first) != 60 {
        return Err("the first round scores or actual pot were not retained".into());
    }
    let rejected =
        send_file_control_request(alice, FileControlAction::DealNextRound, Some(TIMEOUT))?;
    if rejected.status != FileControlStatus::Rejected
        || !rejected
            .error
            .as_deref()
            .is_some_and(|error| error.contains("only the next dealer"))
        || rejected.observation.game != first.game
        || rejected.observation.rounds != first.rounds
        || rejected.observation.coins != first.coins
    {
        return Err(format!(
            "wrong-dealer action was not rejected without mutation: {:?}",
            rejected.error
        ));
    }
    let before_deal = capture(bob)?;
    let dealer_view = send(
        bob,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation;
    let [deck_x, deck_y] = dealer_view
        .camera
        .and_then(|camera| camera.deck_screen)
        .ok_or("dealer cannot see the shared deck")?;
    click(bob, deck_x, deck_y)?;
    let mut second = wait_until(alice, |o| {
        o.game.as_ref().is_some_and(|game| {
            game.round_index == 1 && game.phase == "bidding" && game.hand_size == 2
        }) && o.own_hand.len() == 2
            && o.card_poses.len() == 4
    })?;
    let peer = wait_until(bob, |o| o.game == second.game && o.own_hand.len() == 2)?;
    let second_game = second.game.as_ref().ok_or("second deal missing")?;
    if second_game.dealer_seat != Some(1)
        || second_game.trump.is_none()
        || second_game.scores != first_row.totals
        || second_game.pot_cents != 60
        || !second.revealed_cards.is_empty()
        || !peer.revealed_cards.is_empty()
        || second.rounds != first.rounds
        || second.payment_due_cents != [0, 0]
    {
        return Err(
            "second deal failed private-hand, trump, scores, history, or paid-pot invariants"
                .into(),
        );
    }
    let mut faces: Vec<_> = second
        .own_hand
        .iter()
        .chain(&peer.own_hand)
        .map(|card| &card.face)
        .collect();
    faces.sort();
    faces.dedup();
    if faces.len() != 4 {
        return Err("second deal duplicated a private card face".into());
    }
    let dealt = capture(bob)?;
    for _ in 0..2 {
        let actor = second
            .game
            .as_ref()
            .and_then(|game| game.actor_seat)
            .ok_or("second-round bid has no actor")?;
        let previous = second.game.as_ref().unwrap().action_count;
        send(
            root_for_seat(actor, alice, bob),
            FileControlAction::Bid { tricks: 0 },
        )?;
        second = wait_until(alice, |o| {
            o.game
                .as_ref()
                .is_some_and(|game| game.action_count > previous)
        })?;
    }
    for _ in 0..4 {
        let actor = second
            .game
            .as_ref()
            .and_then(|game| game.actor_seat)
            .ok_or("second-round play has no actor")?;
        let actor_root = root_for_seat(actor, alice, bob);
        let actor_view = send(
            actor_root,
            FileControlAction::Observe {
                include_join_code: false,
            },
        )?
        .observation;
        let previous = second.game.as_ref().unwrap().action_count;
        // The actor sees only their own hand. Try its exposed card actions; the
        // authority's follow-suit rejection must be observable, not time out.
        let mut accepted = false;
        for card_index in 0..actor_view.own_hand.len() {
            let played = send_file_control_request(
                actor_root,
                FileControlAction::PlayOwnCard { card_index },
                Some(TIMEOUT),
            )?;
            if played.status == FileControlStatus::Completed {
                accepted = true;
                break;
            }
            if played.observation.game != actor_view.game {
                return Err("a denied play mutated the public round".into());
            }
        }
        if !accepted {
            return Err("second-round actor had no accepted play from their private hand".into());
        }
        second = wait_until(alice, |o| {
            o.game
                .as_ref()
                .is_some_and(|game| game.action_count > previous)
        })?;
    }
    let scored = wait_until(alice, |o| {
        o.rounds.len() == 2
            && o.own_hand.is_empty()
            && o.game
                .as_ref()
                .is_some_and(|game| game.phase == "scoring" && game.round_index == 1)
    })?;
    wait_until(bob, |o| o.game == scored.game && o.rounds == scored.rounds)?;
    let second_row = &scored.rounds[1];
    let expected_scores = [
        first_row.totals[0] + second_row.points[0],
        first_row.totals[1] + second_row.points[1],
    ];
    if scored.rounds[0] != first_row
        || second_row.totals != expected_scores
        || scored.game.as_ref().unwrap().scores != expected_scores
        || second_row.hand_size != 2
        || second_row.dealer_seat != 1
        || second_row.tricks_won.iter().sum::<u8>() != 2
        || scored.payment_due_cents != second_row.payment_cents
    {
        return Err(
            "second round did not append exactly one correct cumulative scoresheet row".into(),
        );
    }
    let scored_capture = capture(alice)?;
    let due = scored.payment_due_cents;
    for (seat, amount) in due.into_iter().enumerate() {
        if !matches!(amount, 0 | 10) {
            return Err(
                "second round requested something other than one dime per missed bid".into(),
            );
        }
        if amount != 0 {
            drag_coin_to_bowl(root_for_seat(u8::try_from(seat).unwrap(), alice, bob), 10)?;
        }
    }
    let expected_pot = 60 + due.iter().sum::<u32>();
    let paid = wait_until(alice, |o| {
        o.game.as_ref().is_some_and(|game| {
            game.phase == "awaiting-deal"
                && game.round_index == 2
                && game.dealer_seat == Some(0)
                && game.pot_cents == expected_pot
        }) && o.payment_due_cents == [0, 0]
    })?;
    let peer = wait_until(bob, |o| {
        o.game == paid.game && o.rounds == paid.rounds && bowl_cents(o) == expected_pot
    })?;
    for view in [&paid, &peer] {
        if view.rounds != scored.rounds
            || view.game.as_ref().unwrap().scores != expected_scores
            || bowl_cents(view) != expected_pot
            || view.coins.len() != 400
            || view
                .coins
                .iter()
                .map(|coin| u32::from(coin.denomination_cents))
                .sum::<u32>()
                != 7_000
        {
            return Err(
                "round settlement duplicated scores/payments or changed the conserved inventory"
                    .into(),
            );
        }
    }
    let settled = capture(alice)?;
    Ok((vec![before_deal, dealt, scored_capture, settled], paid))
}

fn drag_coin_to_bowl(root: &Path, denomination: u8) -> Result<(), String> {
    let ready = wait_until(root, |o| {
        o.money_bowl_screen.is_some()
            && o.money_pick_targets.iter().any(|target| {
                target.container == "lid" && target.denomination_cents == denomination
            })
    })?;
    let target = ready
        .money_pick_targets
        .iter()
        .filter(|target| target.container == "lid" && target.denomination_cents == denomination)
        // Prefer the clearly exposed upper row in the fixed acceptance camera,
        // not map enumeration order. Still verify actual normal picking below.
        .min_by(|left, right| left.screen[1].total_cmp(&right.screen[1]))
        .ok_or("no camera-visible coin of the required denomination on this player's lid")?;
    let coin = ready
        .coins
        .iter()
        .find(|coin| coin.is_own && coin.coin_id == target.coin_id)
        .ok_or("visible payment coin is missing from the inventory")?;
    // A prior return animation can move this coin between observing its target
    // and pressing. Reproject its rendered center for each input, then confirm
    // the actual picked object before beginning the payment drag.
    send(
        root,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: false,
        },
    )?;
    let grabbed = send(
        root,
        FileControlAction::PointerAtCoin {
            coin_key: coin.coin_key.clone(),
            primary_down: true,
        },
    )?;
    if grabbed
        .observation
        .held_coin
        .as_ref()
        .map(|held| &held.coin_key)
        != Some(&coin.coin_key)
    {
        return Err(format!(
            "payment pointer did not grab selected {}c coin {}",
            denomination, coin.coin_id
        ));
    }
    let [bowl_x, bowl_y] = ready
        .money_bowl_screen
        .ok_or("bowl is outside the camera")?;
    for (x, y, primary_down) in [(bowl_x, bowl_y, true), (bowl_x, bowl_y, false)] {
        send(root, FileControlAction::Pointer { x, y, primary_down })?;
    }
    wait_until(root, |o| {
        o.coins
            .iter()
            .any(|coin| coin.is_own && coin.coin_id == target.coin_id && coin.container == "bowl")
    })?;
    Ok(())
}

fn verify_sheet_inspection(root: &Path) -> Result<Vec<PathBuf>, String> {
    let before = capture(root)?;
    let camera = send(
        root,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation
    .camera
    .ok_or("missing camera diagnostics")?;
    let [x, y] = camera
        .sheet_screen
        .ok_or("scoresheet is outside the world camera")?;
    click(root, x, y)?;
    wait_until(root, |o| {
        o.camera.as_ref().is_some_and(|c| {
            c.inspecting_sheet
                && (c.pitch - std::f32::consts::FRAC_PI_2).abs() < 0.001
                && c.orthographic_scale < 0.25
        })
    })?;
    let inspecting = capture(root)?;
    send(
        root,
        FileControlAction::CameraGesture {
            delta: [12., 8.],
            middle_down: true,
            right_down: false,
        },
    )?;
    send(
        root,
        FileControlAction::CameraGesture {
            delta: [0., 0.],
            middle_down: false,
            right_down: false,
        },
    )?;
    wait_until(root, |o| {
        o.camera.as_ref().is_some_and(|c| {
            c.inspecting_sheet && c.mode == "orthographic" && (c.focus[0] - 0.265).abs() > 0.001
        })
    })?;
    // Projection and angle controls work inside the same inspection bookmark;
    // neither is a dismissal nor a hidden extra tactical mode.
    for (key, mode, top_down) in [
        ("I", "perspective", true),
        ("O", "perspective", false),
        ("I", "orthographic", false),
        ("O", "orthographic", true),
    ] {
        send(
            root,
            FileControlAction::Key {
                key: key.into(),
                down: true,
            },
        )?;
        send(
            root,
            FileControlAction::Key {
                key: key.into(),
                down: false,
            },
        )?;
        wait_until(root, |o| {
            o.camera.as_ref().is_some_and(|c| {
                let angle_ok = if top_down {
                    (c.pitch - std::f32::consts::FRAC_PI_2).abs() < 0.001
                } else {
                    (c.pitch - camera.pitch).abs() < 0.001
                };
                c.inspecting_sheet && c.mode == mode && angle_ok
            })
        })?;
    }
    let adjusted = send(
        root,
        FileControlAction::Observe {
            include_join_code: false,
        },
    )?
    .observation
    .camera
    .ok_or("missing inspected camera")?;
    let [x, y] = adjusted
        .sheet_screen
        .ok_or("panned scoresheet is not visible")?;
    click(root, x, y)?;
    wait_until(root, |o| {
        o.camera.as_ref().is_some_and(|c| {
            !c.inspecting_sheet
                && c.mode == camera.mode
                && c.focus
                    .iter()
                    .zip(camera.focus)
                    .all(|(a, b)| (*a - b).abs() < 0.001)
                && (c.distance - camera.distance).abs() < 0.001
                && (c.pitch - camera.pitch).abs() < 0.001
        })
    })?;
    let restored = capture(root)?;
    Ok(vec![before, inspecting, restored])
}

fn click(root: &Path, x: f32, y: f32) -> Result<(), String> {
    send(
        root,
        FileControlAction::Pointer {
            x,
            y,
            primary_down: false,
        },
    )?;
    send(
        root,
        FileControlAction::Pointer {
            x,
            y,
            primary_down: true,
        },
    )?;
    send(
        root,
        FileControlAction::Pointer {
            x,
            y,
            primary_down: false,
        },
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn captured_card() -> FileControlCardPose {
        // Actual pose from the failed two-viewer run; disposable identity/key
        // values are replaced, while geometry, sequence and logical zone stay.
        FileControlCardPose {
            card_key: "room:alice:card-0-0".into(),
            card_id: "card-0-0".into(),
            owner: "alice".into(),
            owner_seat: 0,
            logical_location: "won:1".into(),
            position_mm: [18, 41, -300],
            rotation_mdeg: [0, 0, 0],
            sequence: 8,
            is_own: true,
        }
    }

    #[test]
    fn canonical_card_convergence_ignores_only_viewer_relative_ownership() {
        let alice = captured_card();
        let mut bob = alice.clone();
        bob.is_own = false;
        assert_ne!(
            alice, bob,
            "whole projection equality caused the live timeout"
        );
        assert!(same_canonical_card(&alice, &bob));
        assert!(same_canonical_card(&alice, &alice));
    }

    #[test]
    fn canonical_card_convergence_still_rejects_a_one_millimeter_pose_difference() {
        let alice = captured_card();
        let mut bob = alice.clone();
        bob.is_own = false;
        bob.position_mm[0] += 1;
        assert!(!same_canonical_card(&alice, &bob));
    }

    #[test]
    fn canonical_card_convergence_still_checks_sequence_and_logical_authority() {
        let alice = captured_card();
        let mut stale = alice.clone();
        stale.is_own = false;
        stale.sequence -= 1;
        assert!(!same_canonical_card(&alice, &stale));
        let mut wrong_winner = alice.clone();
        wrong_winner.is_own = false;
        wrong_winner.logical_location = "won:0".into();
        assert!(!same_canonical_card(&alice, &wrong_winner));
    }
}

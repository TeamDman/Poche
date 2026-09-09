//! Opt-in two-process production connector acceptance; no graphical windows.
//! Files coordinate the test/invitation only. All player actions use Veilid.
use super::*;
use std::{
    path::Path,
    process::{Child, Command},
    time::Instant,
};

struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn opt_in() {
    assert_eq!(
        std::env::var("POCHE_ALLOW_VEILID_PUBLIC_TEST").as_deref(),
        Ok("I_ACCEPT_PUBLIC_NETWORK_TRAFFIC")
    );
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(150);
    while !predicate() {
        assert!(Instant::now() < deadline, "two-process probe timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn invoke(live: &mut NativeLiveDevice, action: &str) {
    wait_until(|| {
        live.poll().expect("device observation");
        live.observation()
            .actions
            .iter()
            .any(|item| item.id == action)
    });
    let before = live.observation().projection.current_revision;
    live.submit_action(action)
        .expect("advertised action submission");
    wait_until(|| {
        live.poll().expect("action observation");
        live.observation().projection.current_revision > before
    });
    assert!(
        matches!(
            live.last_result(),
            Some(poche_player_client::DeviceActionResult::Committed { .. })
        ),
        "action did not receive a committed response"
    );
}

#[test]
#[ignore = "child role for explicitly opted-in real-network process probe"]
fn protected_desktop_process_role() {
    opt_in();
    let root = std::env::var("POCHE_PROCESS_PROBE_ROOT").expect("parent probe directory");
    let root = Path::new(&root);
    let role = std::env::var("POCHE_PROCESS_PROBE_ROLE").expect("parent role");
    let suffix = std::env::var("POCHE_PROCESS_PROBE_SUFFIX").expect("parent suffix");
    let creator = role == "creator" || role == "creator-resume" || role == "alone" || role == "creator-expire" || role == "creator-after-expiry";
    let creator_loss = std::env::var("POCHE_PROCESS_PROBE_CREATOR_LOSS").as_deref() == Ok("1");
    let restarting = role.ends_with("-resume");
    assert!(creator || role == "joiner" || restarting);
    let profile_role = if creator { "creator" } else if restarting { "joiner" } else { &role };
    let name = format!("process-{profile_role}-{suffix}");
    let request = if creator {
        DesktopMenuRequest::Create { name }
    } else {
        wait_until(|| root.join("invitation-ready").exists());
        let invitation = fs::read_to_string(root.join("invitation")).unwrap();
        assert!((validator().0)(&invitation));
        DesktopMenuRequest::Join { name, invitation }
    };
    let mut owners = Vec::new();
    let result = connect(request, &mut owners);
    if role == "creator-expire" {
        assert!(matches!(result, Err("Recovery expired or persistence failed.")), "recovery without peers must expire");
        let label = format!("desktop-{}", &blake3::hash(format!("process-creator-{suffix}").as_bytes()).to_hex()[..24]);
        let store = ProtectedProfileStore::open_default().unwrap();
        let bytes = store.load_authority_recovery(&label, "active-room").unwrap().unwrap();
        assert!(poche_veilid::DesktopRoomDisbanded::decode(&bytes).is_ok(), "expiry did not persist disbanding");
        fs::write(root.join("all-peer-expired"), b"verified").unwrap();
        return;
    }
    let mut live = result.expect("production process connection");
    if role == "creator-after-expiry" {
        assert!(live.room_invitation().unwrap() != fs::read_to_string(root.join("invitation")).unwrap(), "expired room resurrected");
        fs::write(root.join("all-peer-replaced"), b"verified").unwrap();
        return;
    }
    if role == "alone" {
        let path = root.join("previous-invitation");
        let invitation = live.room_invitation().unwrap();
        if path.exists() {
            assert!(fs::read_to_string(&path).unwrap() != invitation, "empty room was resurrected");
            fs::write(root.join("empty-room-replaced"), b"verified").unwrap();
        }
        fs::write(path, invitation).unwrap();
        return;
    }
    if restarting {
        wait_until(|| {
            live.poll().expect("restarted observation");
            live.observation().projection.payload.own_hand.as_ref().is_some_and(|hand| !hand.cards.is_empty())
        });
        assert!(live.observation().projection.payload.members.iter().any(|member| member.principal_id == live.observation().projection.principal_id && member.seat == Some(if creator { 0 } else { 1 })));
        assert!(live.observation().physical_hands.iter().any(|card| card.face.is_some() != creator && card.pose.is_some()));
        if creator {
            assert!(live.room_invitation().unwrap() == fs::read_to_string(root.join("invitation")).unwrap());
            let digest = recovery_digest(&live);
            assert!(digest == fs::read_to_string(root.join("creator-state-digest")).unwrap(), "recovered creator state differs");
        }
        fs::write(root.join(if creator { "creator-resumed" } else { "joiner-resumed" }), b"verified").unwrap();
        return;
    }
    if creator {
        // Never print the bearer secret. TempDir owns its cleanup in the parent.
        fs::write(root.join("invitation"), live.room_invitation().unwrap()).unwrap();
    }
    invoke(
        &mut live,
        if creator {
            "room-take-seat-0"
        } else {
            "room-take-seat-1"
        },
    );
    invoke(&mut live, "room-ready");
    if creator {
        fs::write(root.join("invitation-ready"), b"ready").unwrap();
        invoke(&mut live, "countdown-arm");
    }
    wait_until(|| {
        live.poll().expect("deal observation");
        live.observation()
            .projection
            .payload
            .own_hand
            .as_ref()
            .is_some_and(|hand| !hand.cards.is_empty())
    });
    assert!(
        live.observation()
            .projection
            .payload
            .granted_hands
            .is_empty()
    );
    fs::write(root.join(format!("{role}-dealt")), b"dealt").unwrap();
    let revision = live.observation().projection.current_revision;
    let game = live
        .observation()
        .projection
        .payload
        .public_game_state
        .clone();
    let position = [170, 240, -310];
    let rotation = [45000, 12000, 270000];
    if creator {
        wait_until(|| {
            live.poll().expect("remote pose observation");
            live.observation().physical_hands.iter().any(|card| {
                card.face.is_none()
                    && card.pose.as_ref().is_some_and(|pose| {
                        pose.position_mm == position && pose.rotation_millidegrees == rotation
                    })
            })
        });
        assert_eq!(live.observation().projection.current_revision, revision);
        assert_eq!(
            live.observation().projection.payload.public_game_state,
            game
        );
        fs::write(root.join("creator-saw-motion"), b"verified").unwrap();
        if creator_loss {
            fs::write(root.join("creator-state-digest"), recovery_digest(&live)).unwrap();
            fs::write(root.join("creator-ready-for-kill"), b"ready").unwrap();
            wait_until(|| false);
        }
        wait_until(|| root.join("joiner-resumed").exists());
    } else {
        let card = live
            .observation()
            .physical_hands
            .iter()
            .find(|card| card.face.is_some())
            .expect("owned physical card")
            .id
            .clone();
        live.submit_pose(&card, true, position, rotation)
            .expect("native motion submission");
        wait_until(|| {
            live.poll().expect("own pose receipt");
            live.observation().physical_hands.iter().any(|entry| {
                entry.id == card
                    && entry.pose.as_ref().is_some_and(|pose| {
                        pose.position_mm == position && pose.rotation_millidegrees == rotation
                    })
            })
        });
        assert_eq!(live.observation().projection.current_revision, revision);
        assert_eq!(
            live.observation().projection.payload.public_game_state,
            game
        );
        wait_until(|| root.join("creator-saw-motion").exists());
        fs::write(root.join("joiner-motion-done"), b"verified").unwrap();
        // Parent kills this exact child; no graceful disconnect or destructor
        // shutdown may substitute for abrupt process loss in this probe.
        if creator_loss {
            wait_until(|| {
                // Read failures during creator downtime must not resubmit a
                // game action. The worker refreshes its route on later reads.
                if let Err(error) = live.poll() {
                    assert!(error.contains("transport") || error.contains("progress"), "unexpected recovery error: {error}");
                }
                root.join("creator-resumed").exists()
            });
            assert_eq!(live.observation().projection.payload.public_game_state, game);
        } else { wait_until(|| false); }
    }
}

#[test]
#[ignore = "real public Veilid and fresh persistent protected test profiles"]
fn protected_desktop_two_process() {
    run_two_process(false, false);
}

#[test]
#[ignore = "real public Veilid creator crash with protected profiles"]
fn protected_desktop_creator_crash() {
    run_two_process(true, false);
}

#[test]
#[ignore = "real public Veilid all-peer loss and 60-second recovery expiry"]
fn protected_desktop_all_peer_loss() { run_two_process(true, true); }

#[test]
#[ignore = "real public Veilid and protected empty-room lifecycle"]
fn protected_desktop_empty_room_disbands() {
    opt_in();
    let directory = tempfile::tempdir().unwrap();
    let suffix = now().unwrap().to_string();
    for _ in 0..2 {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["protected_desktop_process_role", "--ignored", "--nocapture"])
            .env("POCHE_PROCESS_PROBE_ROOT", directory.path())
            .env("POCHE_PROCESS_PROBE_ROLE", "alone")
            .env("POCHE_PROCESS_PROBE_SUFFIX", &suffix);
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = ChildOwner(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(150);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "empty-room lifecycle process failed");
                break;
            }
            assert!(Instant::now() < deadline, "empty-room lifecycle timed out");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert!(directory.path().join("empty-room-replaced").exists());
}

fn recovery_digest(live: &NativeLiveDevice) -> String {
    let view = live.observation();
    let bytes = serde_json::to_vec(&(&view.projection.payload.own_hand, &view.projection.payload.public_game_state, &view.physical_hands)).unwrap();
    blake3::hash(&bytes).to_hex().to_string()
}

fn run_two_process(creator_loss: bool, all_loss: bool) {
    opt_in();
    let directory = tempfile::tempdir().unwrap();
    let suffix = now().unwrap().to_string();
    let mut children = Vec::new();
    for role in ["creator", "joiner"] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["protected_desktop_process_role", "--ignored", "--nocapture"])
            .env("POCHE_PROCESS_PROBE_ROOT", directory.path())
            .env("POCHE_PROCESS_PROBE_ROLE", role)
            .env("POCHE_PROCESS_PROBE_CREATOR_LOSS", if creator_loss { "1" } else { "0" })
            .env("POCHE_PROCESS_PROBE_SUFFIX", &suffix);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        children.push(ChildOwner(
            command.spawn().expect("spawn independent device process"),
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        if let Some(status) = children[0].0.try_wait().unwrap() {
            assert!(status.success(), "creator failed before participant restart");
        }
        if let Some(status) = children[1].0.try_wait().unwrap() {
            panic!("joiner exited before forced termination: {status}");
        }
        if directory.path().join(if creator_loss { "creator-ready-for-kill" } else { "joiner-motion-done" }).exists()
            && (!all_loss || directory.path().join("joiner-motion-done").exists()) {
            let mut victim = children.remove(if creator_loss { 0 } else { 1 });
            victim.0.kill().expect("terminate owned joiner process");
            assert!(!victim.0.wait().unwrap().success());
            if all_loss {
                let mut peer = children.remove(0);
                peer.0.kill().unwrap();
                assert!(!peer.0.wait().unwrap().success());
            }
            break;
        }
        assert!(Instant::now() < deadline, "joiner did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut restart = Command::new(std::env::current_exe().unwrap());
    restart.args(["protected_desktop_process_role", "--ignored", "--nocapture"])
        .env("POCHE_PROCESS_PROBE_ROOT", directory.path())
        .env("POCHE_PROCESS_PROBE_ROLE", if all_loss { "creator-expire" } else if creator_loss { "creator-resume" } else { "joiner-resume" })
        .env("POCHE_PROCESS_PROBE_CREATOR_LOSS", if creator_loss { "1" } else { "0" })
        .env("POCHE_PROCESS_PROBE_SUFFIX", &suffix);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        restart.creation_flags(0x0800_0000);
    }
    children.push(ChildOwner(restart.spawn().unwrap()));
    for child in &mut children {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "independent device process failed");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "independent process deadline exceeded"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert!(directory.path().join("creator-dealt").exists());
    assert!(directory.path().join("joiner-dealt").exists());
    assert!(directory.path().join("creator-saw-motion").exists());
    assert!(directory.path().join("joiner-motion-done").exists());
    if all_loss {
        assert!(directory.path().join("all-peer-expired").exists());
        restart.env("POCHE_PROCESS_PROBE_ROLE", "creator-after-expiry");
        let mut child = ChildOwner(restart.spawn().unwrap());
        loop {
            if let Some(status) = child.0.try_wait().unwrap() { assert!(status.success()); break; }
            assert!(Instant::now() < deadline, "replacement room timed out");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(directory.path().join("all-peer-replaced").exists());
    } else {
        assert!(directory.path().join(if creator_loss { "creator-resumed" } else { "joiner-resumed" }).exists());
    }
    eprintln!(
        "two independent protected desktop processes dealt and shared private-safe motion; profile suffix {suffix}"
    );
}

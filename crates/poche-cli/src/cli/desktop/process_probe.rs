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
    let creator = role == "creator";
    assert!(creator || role == "joiner");
    let name = format!("process-{role}-{suffix}");
    let request = if creator {
        DesktopMenuRequest::Create { name }
    } else {
        wait_until(|| root.join("invitation-ready").exists());
        let invitation = fs::read_to_string(root.join("invitation")).unwrap();
        assert!((validator().0)(&invitation));
        DesktopMenuRequest::Join { name, invitation }
    };
    let mut owners = Vec::new();
    let mut live = connect(request, &mut owners).expect("production process connection");
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
    if creator {
        wait_until(|| root.join("joiner-dealt").exists());
    }
}

#[test]
#[ignore = "real public Veilid and fresh persistent protected test profiles"]
fn protected_desktop_two_process() {
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
    eprintln!(
        "two independent protected desktop processes dealt successfully; profile suffix {suffix}"
    );
}

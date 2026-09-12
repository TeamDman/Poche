//! Real route retirement only. No mock callback or alternate game transport.
use super::*;

fn poll(live: &mut NativeLiveDevice) {
    if let Err(error) = live.poll() {
        assert!(
            error.contains("transport") || error.contains("progress"),
            "unexpected route-repair failure: {error}"
        );
    }
}

pub(super) fn run(
    root: &Path,
    role: &str,
    creator: bool,
    live: &mut NativeLiveDevice,
    owners: &[RoomOwner],
) {
    fs::write(root.join(format!("{role}-route-ready")), b"ready").unwrap();
    wait_until(|| {
        poll(live);
        root.join("creator-route-ready").exists() && root.join("joiner-route-ready").exists()
    });
    let before = recovery_digest(live);
    let before_payload = live.observation().projection.payload.clone();
    let revision = live.observation().projection.current_revision;
    let session_epoch = live.observation().projection.session_epoch;
    let principal = live.observation().projection.principal_id.clone();
    let room = live.observation().projection.room_id.clone();
    let invitation = live.room_invitation().unwrap().to_owned();
    let successful_observations = live.successful_observation_count();
    if creator {
        let routes = owners
            .iter()
            .find_map(|owner| owner._routes.as_ref())
            .expect("owned live route task");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let epoch = runtime
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(50),
                    routes.retire_current_route_for_acceptance(),
                )
                .await
            })
            .expect("route retirement request deadline")
            .expect("actual upstream route release");
        eprintln!(
            "poche public route probe: retired route epoch {epoch}; waiting for real RouteChange recovery"
        );
        wait_until(|| {
            poll(live);
            match routes.status() {
                poche_veilid::HostRouteStatus::Healthy { route_epoch } => route_epoch > epoch,
                poche_veilid::HostRouteStatus::Recovering { .. } => false,
                state => panic!("public route recovery stopped: {state:?}"),
            }
        });
        fs::write(root.join("route-republished"), b"verified").unwrap();
        eprintln!("poche public route probe: actual callback renewed the same room route");
    }
    wait_until(|| {
        poll(live);
        root.join("route-republished").exists()
    });
    // Require a subsequent successful transport read, not just the last
    // cached projection. The UI bridge intentionally suppresses unchanged
    // snapshots, so their projection IDs may never reach this render adapter.
    wait_until(|| {
        poll(live);
        live.successful_observation_count() > successful_observations
    });
    assert_eq!(
        recovery_digest(live),
        before,
        "route renewal changed authorized game state"
    );
    assert_eq!(live.observation().projection.payload, before_payload);
    assert_eq!(live.observation().projection.current_revision, revision);
    assert_eq!(live.observation().projection.session_epoch, session_epoch);
    assert_eq!(live.observation().projection.principal_id, principal);
    assert_eq!(live.observation().projection.room_id, room);
    assert!(
        live.room_invitation().unwrap() == invitation,
        "route renewal replaced the invitation"
    );
    fs::write(root.join(format!("{role}-route-verified")), b"verified").unwrap();
    wait_until(|| {
        poll(live);
        root.join("creator-route-verified").exists() && root.join("joiner-route-verified").exists()
    });
    // The ordinary two-process probe now continues: signed shared card motion,
    // peer privacy, abrupt participant termination and original-code restart.
}

//! Real mock DHT/route APIs with explicit missing-notification injection.
//! The pinned mock release neither emits `RouteChange` nor kills imported aliases.
use super::*;
use crate::{
    ApplicationIdentity, IdentityStoragePolicy, PublicRoomMetadata, RoomNetwork,
    VeilidProtectedIdentityStore, VeilidRendezvous,
};
use poche_protocol::RoomId;
use std::{str::FromStr, sync::Arc};
use veilid_core::{VeilidConfig, VeilidRouteChange, VeilidUpdate};

fn death(node: &VeilidDeviceNode, route: RouteId) {
    node.simulate_route_change_for_mock(VeilidRouteChange {
        dead_routes: vec![route.clone(), route],
        dead_remote_routes: vec![],
    });
}

async fn healthy_at(task: &RunningHostRoute, epoch: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match task.status() {
                HostRouteStatus::Healthy { route_epoch } if route_epoch == epoch => break,
                HostRouteStatus::Failed(error) => panic!("route task failed: {error:?}"),
                HostRouteStatus::Stopped => panic!("route task stopped prematurely"),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .expect("automatic route recovery deadline");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One continuous new -> live repair -> restored lifecycle.
async fn callback_driven_new_and_resumed_routes_keep_the_original_invitation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_str().unwrap();
    let mut config = VeilidConfig::new(
        "poche_live_route",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    config.namespace = "owner".into();
    let (send, mut calls) = tokio::sync::mpsc::channel(4);
    let server = VeilidDeviceNode::start(
        config.clone(),
        Arc::new(move |update| {
            if let VeilidUpdate::AppCall(call) = update {
                let _ = send.try_send(call);
            }
        }),
    )
    .unwrap();
    let updates = server.local_route_updates(); // before allocation/publication
    config.namespace = "peer".into();
    let peer = VeilidDeviceNode::start(config, Arc::new(drop)).unwrap();
    server.api().attach().await.unwrap();
    peer.api().attach().await.unwrap();
    let api = server.api().clone();
    let echo = server.runtime().spawn(async move {
        while let Some(call) = calls.recv().await {
            api.app_call_reply(call.id(), call.message().to_vec())
                .await
                .unwrap();
        }
    });
    let host = VeilidRendezvous::new(server.api().clone()).unwrap();
    let client = VeilidRendezvous::new(peer.api().clone()).unwrap();
    let identity = ApplicationIdentity::load_or_create(
        &VeilidProtectedIdentityStore::new(server.api().clone()),
        IdentityStoragePolicy::RequireProtected,
    )
    .await
    .unwrap();
    let published = host
        .publish_room(
            &identity,
            RoomNetwork::VeilidLocal,
            RoomId::new("automatic-route-recovery").unwrap(),
            PublicRoomMetadata::new("route test", 2, true).unwrap(),
            4,
            7,
            u64::MAX,
            100,
        )
        .await
        .unwrap();
    let invitation = published.room_code().encode().unwrap();
    let code = crate::RoomCode::decode(invitation.expose(), 100).unwrap();
    let original_record = published.record().clone();
    let original_route = published.route_id().clone();
    let mut resolved = client.resolve_room(&code, 101).await.unwrap();
    assert_eq!(
        client
            .app_call(&resolved, b"before".to_vec())
            .await
            .unwrap(),
        b"before"
    );
    // These are genuine mock route releases. Its missing callbacks and alias
    // invalidation are injected explicitly, not claimed to be upstream faults.
    server
        .api()
        .release_private_route(original_route.clone())
        .unwrap();
    peer.api()
        .release_private_route(resolved.route_id().clone())
        .unwrap();
    assert!(client.app_call(&resolved, b"stale".to_vec()).await.is_err());
    death(&server, original_route.clone()); // queued before task registration
    let running =
        RunningHostRoute::start(server.clone(), host.own_published_route(published), updates);
    healthy_at(&running, 8).await;
    client
        .refresh_resolved_room(&mut resolved, 102)
        .await
        .unwrap();
    assert_eq!(resolved.record().route_epoch, 8);
    assert_eq!(resolved.record().room_id, original_record.room_id);
    assert_eq!(
        resolved.record().host_identity,
        original_record.host_identity
    );
    assert_eq!(
        resolved.record().session_epoch,
        original_record.session_epoch
    );
    assert_eq!(
        resolved.record().expires_at_unix_ms,
        original_record.expires_at_unix_ms
    );
    assert_eq!(
        client.app_call(&resolved, b"after".to_vec()).await.unwrap(),
        b"after"
    );
    // Delayed duplicate death, unrelated routes and imported-route deaths do
    // not rotate the owner's healthy current route.
    death(&server, original_route);
    server.simulate_route_change_for_mock(VeilidRouteChange {
        dead_routes: vec![],
        dead_remote_routes: vec![resolved.route_id().clone()],
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(
        running.status(),
        HostRouteStatus::Healthy { route_epoch: 8 }
    );
    let fresh = client.resolve_room(&code, 103).await.unwrap();
    let local = RouteId::from_str(
        std::str::from_utf8(&fresh.record().private_route_blob().unwrap()).unwrap(),
    )
    .unwrap();
    running.stop().await.unwrap();
    assert!(
        server.api().release_private_route(local).is_err(),
        "stop released the active route"
    );

    // Restoration reuses the protected owner capability; its live maintenance
    // is identical and must not require another invitation or identity.
    let updates = server.local_route_updates();
    let resumed = host
        .resume_host_room(&identity, &original_record.room_id, u64::MAX, 104)
        .await
        .unwrap();
    let route = resumed.route_id().clone();
    let running = RunningHostRoute::start(server.clone(), host.own_resumed_route(resumed), updates);
    server.api().release_private_route(route.clone()).unwrap();
    death(&server, route);
    healthy_at(&running, 10).await;
    let final_view = client.resolve_room(&code, 105).await.unwrap();
    assert_eq!(final_view.record().room_id, original_record.room_id);
    assert_eq!(final_view.record().route_epoch, 10);
    assert_eq!(
        client
            .app_call(&final_view, b"restored".to_vec())
            .await
            .unwrap(),
        b"restored"
    );
    running.stop().await.unwrap();
    echo.abort();
    let _ = echo.await;
    drop(host);
    drop(client);
    server.shutdown().unwrap();
    peer.shutdown().unwrap();
}

#[tokio::test]
async fn notification_overflow_is_bounded_and_does_not_lose_route_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_str().unwrap();
    let config = VeilidConfig::new(
        "poche_route_overflow",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    let node = VeilidDeviceNode::start(config, Arc::new(drop)).unwrap();
    let updates = node.local_route_updates();
    node.api().attach().await.unwrap();
    let adapter = VeilidRendezvous::new(node.api().clone()).unwrap();
    let identity = ApplicationIdentity::load_or_create(
        &VeilidProtectedIdentityStore::new(node.api().clone()),
        IdentityStoragePolicy::RequireProtected,
    )
    .await
    .unwrap();
    let published = adapter
        .publish_room(
            &identity,
            RoomNetwork::VeilidLocal,
            RoomId::new("lagged-route-owner").unwrap(),
            PublicRoomMetadata::new("route test", 2, true).unwrap(),
            1,
            1,
            u64::MAX,
            100,
        )
        .await
        .unwrap();
    let route = published.route_id().clone();
    for _ in 0..80 {
        death(&node, route.clone());
    }
    let task = RunningHostRoute::start(
        node.clone(),
        adapter.own_published_route(published),
        updates,
    );
    healthy_at(&task, 2).await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(
        task.status(),
        HostRouteStatus::Healthy { route_epoch: 2 },
        "duplicate events must coalesce"
    );
    task.stop().await.unwrap();
    drop(adapter);
    node.shutdown().unwrap();
}

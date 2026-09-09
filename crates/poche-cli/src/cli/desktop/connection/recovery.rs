//! Protected same-device creator restoration. Runs on the menu worker only.
use super::*;

/// A creator alone in the authenticated snapshot has no other peer retaining
/// the room. Commit disbanding before allowing a new room to replace the slot.
/// Corruption or a mismatched creator is never interpreted as an empty room.
pub(super) fn terminal_if_no_peers(bytes: &[u8], creator: &poche_protocol::PrincipalId) -> Result<Option<Vec<u8>>, &'static str> {
    let (genesis, room) = poche_veilid::DesktopRoomGenesis::decode_recovery(bytes)
        .map_err(|_| "Saved lobby recovery state is invalid or closed.")?;
    let peers = room.recovery_peers(creator).map_err(|_| "Saved lobby belongs to a different creator.")?;
    if peers.is_empty() {
        poche_veilid::DesktopRoomDisbanded::encode(genesis.room_id()).map(Some)
            .map_err(|_| "Cannot prepare previous lobby disbanding.")
    } else { Ok(None) }
}

pub(super) fn restore(
    node: VeilidDeviceNode,
    profile: poche_player_client::DeviceProfile,
    store: ProtectedProfileStore,
    bytes: &[u8],
    label: &str,
    incoming: tokio::sync::mpsc::Receiver<Box<veilid_core::VeilidAppCall>>,
    lease: fs::File,
    owners: &mut Vec<RoomOwner>,
) -> Result<NativeLiveDevice, &'static str> {
    use poche_player_client::DeviceClientError;
    use poche_veilid::{DesktopRoomGenesis, DesktopRoomDisbanded, VeilidRendezvous};
    let recovery_store = std::sync::Arc::new(std::sync::Mutex::new(
        ProtectedProfileStore::open_default().map_err(|_| "Cannot open recovery storage.")?));
    let writer_store = recovery_store.clone();
    let writer_label = label.to_owned();
    let expiry_label = label.to_owned();
    // Decode before touching the old DHT owner capability. Corrupt state never
    // falls through to publishing a replacement lobby.
    let (metadata, _) = DesktopRoomGenesis::decode_recovery(bytes)
        .map_err(|_| "Saved lobby recovery state is invalid or closed.")?;
    let terminal = DesktopRoomDisbanded::encode(metadata.room_id())
        .map_err(|_| "Cannot prepare room expiry state.")?;
    let (genesis, service) = DesktopRoomGenesis::recovered_service(
        bytes, &profile.player_id, Duration::from_secs(60),
        move |genesis, state| {
            let bytes = zeroize::Zeroizing::new(genesis.encode_recovery(state)?);
            writer_store.lock().map_err(|_| DeviceClientError::TransportUnavailable)?
                .save_authority_recovery(&writer_label, "active-room", &bytes)
                .map_err(|_| DeviceClientError::TransportUnavailable)
        },
        move || recovery_store.lock().map_err(|_| DeviceClientError::TransportUnavailable)?
            .save_authority_recovery(&expiry_label, "active-room", &terminal)
            .map_err(|_| DeviceClientError::TransportUnavailable),
    ).map_err(|_| "Saved lobby has no eligible surviving peer or cannot be restored.")?;
    let invitation = genesis.invitation(now()?).and_then(|code| code.encode().map_err(|_| DeviceClientError::ProtocolViolation))
        .map_err(|_| "Saved invitation is invalid or expired.")?.expose().to_owned();
    let identity = node.runtime().block_on(ApplicationIdentity::load_or_create(
        &VeilidProtectedIdentityStore::new(node.api().clone()), IdentityStoragePolicy::RequireProtected,
    )).map_err(|_| "Cannot recover node identity.")?;
    let adapter = VeilidRendezvous::new(node.api().clone()).map_err(|_| "Cannot initialize recovery routing.")?;
    let resumed = node.runtime().block_on(adapter.resume_host_room(&identity, genesis.room_id(), u64::MAX, now()?))
        .map_err(|_| "Cannot restore the original room route.")?;
    let monitor = service.clone();
    let running = service.serve(node.clone(), incoming);
    let result = (|| {
        let started = std::time::Instant::now();
        while !monitor.recovery_ready().map_err(|_| "Recovery expired or persistence failed.")? {
            if started.elapsed() > Duration::from_secs(65) { return Err("No surviving peer answered recovery."); }
            std::thread::sleep(Duration::from_millis(100));
        }
        let (client, room_id) = poche_veilid::join_device(node.clone(), profile, store, &invitation, now()?)
            .map_err(|_| "The lobby recovered, but this device could not reconnect.")?;
        let mut live = NativeLiveDevice::connect(client, room_id).map_err(|_| "Cannot load recovered table.")?;
        live.set_room_invitation(invitation);
        Ok(live)
    })();
    match result {
        Ok(live) => {
            owners.push(RoomOwner { _resumed: Some(resumed), _publication: None, _service: Some(running), _lease: lease });
            Ok(live)
        }
        Err(error) => {
            drop(running);
            let _ = node.runtime().block_on(resumed.close(node.api()));
            Err(error)
        }
    }
}

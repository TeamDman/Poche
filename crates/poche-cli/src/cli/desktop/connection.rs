use poche_native_ui::{
    NativeLiveDevice,
    desktop_menu::{DesktopConnectionWorker, DesktopMenuRequest, InvitationValidator},
};
use poche_player_client::ProtectedProfileStore;
use poche_veilid::{
    ApplicationIdentity, IdentityStoragePolicy, PublishedRoom, RoomCode, RoomNetwork,
    RunningDeviceService, VeilidDeviceNode, VeilidProtectedIdentityStore,
};
use std::{
    fs,
    net::UdpSocket,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now() -> Result<u64, &'static str> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok())
        .ok_or("System clock is invalid.")
}

pub fn validator() -> InvitationValidator {
    InvitationValidator(|text| {
        now()
            .ok()
            .is_some_and(|now| RoomCode::decode(text, now).is_ok())
    })
}

// Retained by the worker closure for the whole graphical app lifetime. This
// is initial room service ownership, not replica-based creator failover.
struct RoomOwner {
    _publication: Option<PublishedRoom>,
    _service: Option<RunningDeviceService>,
    _lease: fs::File,
}

pub fn worker() -> Result<DesktopConnectionWorker, &'static str> {
    let mut owners = Vec::<RoomOwner>::new();
    DesktopConnectionWorker::start(move |request| {
        let (name, invitation) = match request {
            DesktopMenuRequest::Create { name } => (name, None),
            DesktopMenuRequest::Join { name, invitation } => (name, Some(invitation)),
        };
        let now = now()?;
        if let Some(code) = &invitation {
            let decoded =
                RoomCode::decode(code, now).map_err(|_| "Invalid or expired invitation.")?;
            if decoded.network() != RoomNetwork::VeilidPublic {
                return Err("This client requires a public Veilid invitation.");
            }
        }
        // Name selects a protected local profile; it is never remote identity
        // proof. Distinct names permit two independent windows on this device.
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
            return Err("Enter a valid player name.");
        }
        let label = format!("desktop-{}", &blake3::hash(name.as_bytes()).to_hex()[..24]);
        let store = ProtectedProfileStore::open_default()
            .map_err(|_| "Protected credential storage is unavailable.")?;
        let directory = store.public_root().join("nodes").join(&label);
        fs::create_dir_all(&directory).map_err(|_| "Cannot open local node storage.")?;
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("instance.lock"))
            .map_err(|_| "Cannot lock local node storage.")?;
        lease.try_lock().map_err(
            |_| "This local player is already open. Choose a different name for a second player.",
        )?;
        let existing = fs::read_dir(&directory)
            .map_err(|_| "Cannot inspect local node storage.")?
            .try_fold(false, |exists, entry| {
                entry.map(|entry| exists || entry.file_name() != "instance.lock")
            })
            .map_err(|_| "Cannot inspect local node storage.")?;
        let profile = store
            .load_or_create_device(&label, &label)
            .map_err(|_| "Cannot recover this player’s protected identity.")?;
        let path = directory
            .to_str()
            .ok_or("Node storage path is unsupported.")?;
        let mut config = veilid_core::VeilidConfig::new(
            "poche_desktop",
            "teamdman",
            "org",
            Some(path),
            Some(path),
        );
        config.namespace = label.clone();
        store
            .with_transport_password(&label, !existing, |password| {
                config.protected_store.device_encryption_key_password = password.to_owned();
            })
            .map_err(|_| "Cannot recover this node’s protected storage credential.")?;
        config.protected_store.allow_insecure_fallback = false;
        config.protected_store.always_use_insecure_storage = false;
        poche_veilid::validate_protected_store_config(&config.protected_store)
            .map_err(|_| "Protected node configuration is invalid.")?;
        let socket =
            UdpSocket::bind("127.0.0.1:0").map_err(|_| "Cannot allocate a network port.")?;
        let port = socket
            .local_addr()
            .map_err(|_| "Cannot allocate a network port.")?
            .port();
        drop(socket);
        config.network.upnp = false;
        config.network.protocol.udp.listen_address = format!("0.0.0.0:{port}");
        config.network.protocol.tcp.listen = false;
        config.network.protocol.ws.listen = false;
        let (send, receive) = tokio::sync::mpsc::channel(32);
        let node = VeilidDeviceNode::start(
            config,
            std::sync::Arc::new(move |update| {
                if let veilid_core::VeilidUpdate::AppCall(call) = update {
                    let _ = send.try_send(call);
                }
            }),
        )?;
        node.runtime()
            .block_on(node.attach_public(Duration::from_secs(90)))?;
        let (client, room_id, code, publication, service) = if let Some(code) = invitation {
            let (client, room_id) = poche_veilid::join_device(node, profile, store, &code, now)
                .map_err(|_| "Could not join this lobby. Check the invitation and connection.")?;
            (client, room_id, code, None, None)
        } else {
            let identity = node
                .runtime()
                .block_on(ApplicationIdentity::load_or_create(
                    &VeilidProtectedIdentityStore::new(node.api().clone()),
                    IdentityStoragePolicy::RequireProtected,
                ))
                .map_err(|_| "Cannot recover the node identity.")?;
            let (published, service) = node
                .runtime()
                .block_on(poche_veilid::publish_device_room(
                    &node,
                    &identity,
                    RoomNetwork::VeilidPublic,
                    "Poche lobby",
                    now,
                    receive,
                ))
                .map_err(|_| "Could not publish the lobby.")?;
            let code = published
                .room_code()
                .encode()
                .map_err(|_| "Cannot encode lobby invitation.")?
                .expose()
                .to_owned();
            let (client, room_id) = poche_veilid::create_device(node, profile, store, &code, now)
                .map_err(|_| "Could not initialize the lobby.")?;
            (client, room_id, code, Some(published), Some(service))
        };
        let mut live = NativeLiveDevice::connect(client, room_id)
            .map_err(|_| "Could not load the live table.")?;
        live.set_room_invitation(code);
        owners.push(RoomOwner {
            _publication: publication,
            _service: service,
            _lease: lease,
        });
        Ok(live)
    })
}

//! Menu/CLI admission through the same certified device actions as gameplay.

use crate::{RoomCode, VeilidDeviceNode, VeilidDeviceTransport, VeilidRendezvous};
use poche_player_client::{
    DeviceActionResult, DeviceClientError, DeviceProfile, DeviceSigner, PlayerDeviceClient,
};
use poche_protocol::{CommandId, CommandPayload, RoomId, RoomPhase};

/// Initialize an already published, pending room through its certified Create
/// action. The caller must retain the published room and serving task; this
/// operation does not itself publish a route or promise creator failover.
/// Like join_device, call from an ordinary connection worker.
pub fn create_device<S: DeviceSigner>(
    node: VeilidDeviceNode,
    profile: DeviceProfile,
    signer: S,
    invitation: &str,
    now_unix_ms: u64,
) -> Result<(PlayerDeviceClient<VeilidDeviceTransport<S>>, RoomId), DeviceClientError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(DeviceClientError::TransportUnavailable);
    }
    let code = RoomCode::decode(invitation, now_unix_ms)
        .map_err(|_| DeviceClientError::AuthorizationDenied)?;
    let adapter = VeilidRendezvous::new(node.api().clone())
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
    let resolved = node
        .runtime()
        .block_on(adapter.resolve_room(&code, now_unix_ms))
        .map_err(|_| {
            eprintln!("poche creator initialization: rendezvous resolution failed");
            DeviceClientError::TransportUnavailable
        })?;
    let room_id = resolved.record().room_id.clone();
    let principal = profile.player_id.clone();
    let transport = VeilidDeviceTransport::from_node(node, resolved, signer, 0, None)?;
    let mut client = PlayerDeviceClient::new(profile, transport)?;
    let pending = client.observe(&room_id).inspect_err(|error| {
        eprintln!("poche creator initialization: initial observation failed ({error})");
    })?;
    let action = pending
        .actions
        .iter()
        .find(|action| matches!(action.payload, CommandPayload::CreateRoom))
        .ok_or(DeviceClientError::AuthorizationDenied)?;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| DeviceClientError::KeyUnavailable)?;
    let command = CommandId::new(format!("create-{}", data_encoding::HEXLOWER.encode(&bytes)))
        .map_err(|_| DeviceClientError::ProtocolViolation)?;
    if !matches!(
        client
            .invoke(&pending, &action.id, command)
            .inspect_err(|error| {
                eprintln!("poche creator initialization: Create invocation failed ({error})");
            })?,
        DeviceActionResult::Committed { .. }
    ) {
        return Err(DeviceClientError::AuthorizationDenied);
    }
    let created = client.observe(&room_id).inspect_err(|error| {
        eprintln!("poche creator initialization: confirmation failed ({error})");
    })?;
    if created.projection.payload.phase != RoomPhase::Lobby
        || !created
            .projection
            .payload
            .members
            .iter()
            .any(|member| member.principal_id == principal && member.connected && member.host)
    {
        return Err(DeviceClientError::InvalidObservation);
    }
    Ok((client, room_id))
}

/// Resolve and join an invitation on an ordinary connection worker. The node
/// must already be attached. Never call this blocking operation in Bevy Update
/// or within Tokio; success retains the node in the returned client.
///
/// An invitation admits a separately certified player. It never substitutes
/// the creator's credentials or turns a pending room into a newly created one.
pub fn join_device<S: DeviceSigner>(
    node: VeilidDeviceNode,
    profile: DeviceProfile,
    signer: S,
    invitation: &str,
    now_unix_ms: u64,
) -> Result<(PlayerDeviceClient<VeilidDeviceTransport<S>>, RoomId), DeviceClientError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(DeviceClientError::TransportUnavailable);
    }
    let code = RoomCode::decode(invitation, now_unix_ms)
        .map_err(|_| DeviceClientError::AuthorizationDenied)?;
    let proof = code
        .admission_proof()
        .map_err(|_| DeviceClientError::AuthorizationDenied)?;
    let adapter = VeilidRendezvous::new(node.api().clone())
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
    let resolved = node
        .runtime()
        .block_on(adapter.resolve_room(&code, now_unix_ms))
        .map_err(|_| DeviceClientError::TransportUnavailable)?;
    let room_id = resolved.record().room_id.clone();
    let epoch = resolved.record().session_epoch;
    let principal = profile.player_id.clone();
    let transport = VeilidDeviceTransport::from_node(node, resolved, signer, epoch, None)?;
    let mut client = PlayerDeviceClient::new(profile, transport)?;
    let mut observation = match client.observe(&room_id) {
        Ok(observation) => observation,
        Err(DeviceClientError::TransportUnavailable) => {
            client.rebind_route(&room_id)?;
            client.observe(&room_id)?
        }
        Err(error) => return Err(error),
    };
    if observation.projection.payload.phase == RoomPhase::Closed {
        return Err(DeviceClientError::AuthorizationDenied);
    }
    if !observation
        .projection
        .payload
        .members
        .iter()
        .any(|member| member.principal_id == principal)
    {
        client.transport_mut().set_join_invite(Some(proof));
        observation = client.observe(&room_id)?;
    }
    if let Some(action) = observation.actions.iter().find(|action| {
        matches!(
            action.payload,
            CommandPayload::RedeemInvite { .. } | CommandPayload::Reconnect
        )
    }) {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| DeviceClientError::KeyUnavailable)?;
        let command = CommandId::new(format!("join-{}", data_encoding::HEXLOWER.encode(&bytes)))
            .map_err(|_| DeviceClientError::ProtocolViolation)?;
        if !matches!(
            client.invoke(&observation, &action.id, command)?,
            DeviceActionResult::Committed { .. }
        ) {
            return Err(DeviceClientError::AuthorizationDenied);
        }
    }
    // No optimistic admission: require a subsequent exact-recipient membership
    // observation before a graphical caller transitions away from the menu.
    let joined = client.observe(&room_id)?;
    if !joined
        .projection
        .payload
        .members
        .iter()
        .any(|member| member.principal_id == principal && member.connected)
    {
        return Err(DeviceClientError::AuthorizationDenied);
    }
    Ok((client, room_id))
}

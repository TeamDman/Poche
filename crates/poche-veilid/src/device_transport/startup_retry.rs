//! Bounded replay of one already-signed startup command, never a replacement.
use super::{DeviceActionRequest, DeviceActionResult, DeviceClientError, VeilidDeviceRequest};
use crate::VeilidRendezvousError;
use poche_protocol::CommandPayload;
use std::time::Duration;

pub(super) fn exchange(
    request: &VeilidDeviceRequest,
    mut send: impl FnMut(&[u8], bool) -> Result<Vec<u8>, VeilidRendezvousError>,
    mut pause: impl FnMut(Duration),
) -> Result<Vec<u8>, DeviceClientError> {
    // Freeze certificate, signature, command ID, payload, epoch and revision
    // together. A refreshed route never causes a new action to be prepared.
    let bytes = request.encode()?;
    let replay_safe = matches!(request, VeilidDeviceRequest::Invoke(action)
        if matches!(action.payload, CommandPayload::CreateRoom
            | CommandPayload::RedeemInvite { .. } | CommandPayload::Reconnect));
    let mut refresh = false;
    for attempt in 0..3 {
        match send(&bytes, refresh) {
            Ok(reply) => return Ok(reply), // Wire denials are not transport loss.
            Err(error) => {
                let retry = matches!(
                    error,
                    VeilidRendezvousError::TryAgain
                        | VeilidRendezvousError::Timeout
                        | VeilidRendezvousError::NoConnection
                        | VeilidRendezvousError::StaleRoute
                        | VeilidRendezvousError::WatchRenewal
                );
                if !replay_safe || !retry || attempt == 2 {
                    eprintln!("poche: device RPC failed ({error:?}); no further submission");
                    return Err(DeviceClientError::TransportUnavailable);
                }
                refresh = matches!(
                    error,
                    VeilidRendezvousError::NoConnection
                        | VeilidRendezvousError::StaleRoute
                        | VeilidRendezvousError::WatchRenewal
                );
                eprintln!(
                    "poche: startup RPC {error:?}; replaying the identical signed command ({}/3)",
                    attempt + 2
                );
                pause(Duration::from_millis(250 * (attempt + 1)));
            }
        }
    }
    unreachable!("the final attempt always returns")
}

pub(super) fn validate_result(
    request: &DeviceActionRequest,
    result: &DeviceActionResult,
) -> Result<(), DeviceClientError> {
    let valid = match result {
        DeviceActionResult::Committed {
            command_id,
            revision,
        } => *command_id == request.command_id && *revision > request.expected_revision,
        DeviceActionResult::Denied { command_id, .. } => *command_id == request.command_id,
    };
    if valid {
        Ok(())
    } else {
        Err(DeviceClientError::ProtocolViolation)
    }
}

#[cfg(all(test, feature = "device-service"))]
mod tests;

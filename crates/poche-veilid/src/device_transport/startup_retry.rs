//! Bounded replay of one already-signed rules command, never a replacement.
//! The historical module name reflects its initial startup-only scope.
use super::{DeviceActionRequest, DeviceActionResult, DeviceClientError, VeilidDeviceRequest};
use crate::VeilidRendezvousError;
use std::time::{Duration, Instant};

pub(super) fn exchange(
    request: &VeilidDeviceRequest,
    mut send: impl FnMut(&[u8], bool) -> Result<Vec<u8>, VeilidRendezvousError>,
    mut pause: impl FnMut(Duration),
) -> Result<Vec<u8>, DeviceClientError> {
    // Freeze certificate, signature, command ID, payload, epoch and revision
    // together. A refreshed route never causes a new action to be prepared.
    let bytes = request.encode()?;
    // CertifiedDeviceRoom durably caches the exact complete signed Invoke
    // before acknowledgement. Other RPCs have different replay contracts.
    let replay_safe = match request {
        VeilidDeviceRequest::Invoke(_) => true,
        VeilidDeviceRequest::Observe(_)
        | VeilidDeviceRequest::PhysicalPose(_)
        | VeilidDeviceRequest::Route(_)
        | VeilidDeviceRequest::Cooperate(_) => false,
    };
    let mut refresh = false;
    for attempt in 0..3 {
        let started = Instant::now();
        match send(&bytes, refresh) {
            Ok(reply) => return Ok(reply), // Wire denials are not transport loss.
            Err(error) => {
                // Static operation/kind only: never dump signed requests,
                // card faces, chat, invitations, or identity material.
                let operation = match request {
                    VeilidDeviceRequest::Invoke(action) => {
                        format!("Invoke({:?})", action.payload.kind())
                    }
                    VeilidDeviceRequest::Observe(_) => "Observe".into(),
                    VeilidDeviceRequest::PhysicalPose(_) => "PhysicalPose".into(),
                    VeilidDeviceRequest::Route(_) => "Route".into(),
                    VeilidDeviceRequest::Cooperate(_) => "Cooperate".into(),
                };
                let elapsed_ms = started.elapsed().as_millis();
                let retry = matches!(
                    error,
                    VeilidRendezvousError::TryAgain
                        | VeilidRendezvousError::Timeout
                        | VeilidRendezvousError::NoConnection
                        | VeilidRendezvousError::StaleRoute
                        | VeilidRendezvousError::WatchRenewal
                );
                if !replay_safe || !retry || attempt == 2 {
                    eprintln!(
                        "poche: {operation} RPC failed ({error:?}, attempt {} in {elapsed_ms}ms); no further submission",
                        attempt + 1
                    );
                    return Err(DeviceClientError::TransportUnavailable);
                }
                refresh = matches!(
                    error,
                    VeilidRendezvousError::NoConnection
                        | VeilidRendezvousError::StaleRoute
                        | VeilidRendezvousError::WatchRenewal
                );
                eprintln!(
                    "poche: {operation} RPC {error:?} after {elapsed_ms}ms; replaying the identical signed command ({}/3)",
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

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic no-socket adapter for CI, headless play, and puppet tests.

use poche_protocol::{DeviceId, RoomId};

use crate::{
    DeviceActionRequest, DeviceActionResult, DeviceClientError, DeviceCooperationRequest,
    DeviceCooperationResult, DeviceObservation, DeviceProfile, DeviceTransport,
};

/// In-process authority surface. Implementations compose the real reducer and
/// exact projection/action derivation; this interface does not offer a state
/// mutation shortcut.
pub trait LoopbackDeviceAuthority {
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError>;

    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError>;

    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError>;

    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError>;
}

/// Concrete no-socket transport using exactly the same public client contract
/// as future gateway and native Veilid transports.
pub struct LoopbackDeviceTransport<A> {
    authority: A,
}

impl<A> LoopbackDeviceTransport<A> {
    #[must_use]
    pub const fn new(authority: A) -> Self {
        Self { authority }
    }

    #[must_use]
    pub const fn authority(&self) -> &A {
        &self.authority
    }

    #[must_use]
    pub fn into_authority(self) -> A {
        self.authority
    }
}

impl<A: LoopbackDeviceAuthority> DeviceTransport for LoopbackDeviceTransport<A> {
    fn observe(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
    ) -> Result<DeviceObservation, DeviceClientError> {
        self.authority.observe(profile, room_id)
    }

    fn invoke(
        &mut self,
        profile: &DeviceProfile,
        request: DeviceActionRequest,
    ) -> Result<DeviceActionResult, DeviceClientError> {
        self.authority.invoke(profile, request)
    }

    fn wait(
        &mut self,
        profile: &DeviceProfile,
        room_id: &RoomId,
        after_revision: u64,
    ) -> Result<DeviceObservation, DeviceClientError> {
        self.authority.wait(profile, room_id, after_revision)
    }

    fn cooperate(
        &mut self,
        profile: &DeviceProfile,
        target_device: &DeviceId,
        request: DeviceCooperationRequest,
    ) -> Result<DeviceCooperationResult, DeviceClientError> {
        self.authority.cooperate(profile, target_device, request)
    }
}

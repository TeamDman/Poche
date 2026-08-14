// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use eyre::{Result, eyre};
use poche_player_client::{
    AdvertisedAction, DeviceActionResult, DeviceObservation, HttpDeviceTransport,
    PlayerDeviceClient, ProtectedProfileStore,
};
use poche_protocol::{CommandId, CommandPayload, InviteProof, RoomId};

use super::{GlobalArgs, output::OutputFormat};
use crate::cli::output::emit_value;

const DEFAULT_DEVICE_ENDPOINT: &str = "http://127.0.0.1:4174";

pub struct LiveDeviceConfig {
    endpoint: String,
    profile: Option<String>,
}

impl LiveDeviceConfig {
    pub fn from_global(global: &GlobalArgs) -> Self {
        Self {
            endpoint: global
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_DEVICE_ENDPOINT.to_owned()),
            profile: global.profile.clone(),
        }
    }

    pub fn observe(&self, room: &str, epoch: u64, output: OutputFormat) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(epoch)?;
        let observation = client.observe(&room_id)?;
        emit_observation(&observation, output)?;
        Ok(true)
    }

    pub fn actions(&self, room: &str, epoch: u64, output: OutputFormat) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(epoch)?;
        let observation = client.observe(&room_id)?;
        let text = actions_text(&observation.actions);
        emit_value(&observation.actions, &text, output)?;
        Ok(true)
    }

    pub fn invoke_payload(
        &self,
        room: &str,
        epoch: u64,
        payload: &CommandPayload,
        command_prefix: &str,
        output: OutputFormat,
    ) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(epoch)?;
        let observation = client.observe(&room_id)?;
        let result =
            client.invoke_payload(&observation, payload, random_command_id(command_prefix)?)?;
        let text = action_result_text(&result);
        emit_value(&result, &text, output)?;
        Ok(true)
    }

    pub fn invoke_countdown(&self, room: &str, ticks: u64, output: OutputFormat) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(1)?;
        let observation = client.observe(&room_id)?;
        let action = observation
            .action("countdown-arm")
            .ok_or_else(|| eyre!("countdown is not currently advertised"))?;
        if !matches!(
            action.payload,
            CommandPayload::ArmCountdown { deadline_tick, .. } if deadline_tick == ticks
        ) {
            return Err(eyre!(
                "requested countdown differs from the authority-advertised countdown"
            ));
        }
        let result = client.invoke(
            &observation,
            "countdown-arm",
            random_command_id("room-countdown")?,
        )?;
        let text = action_result_text(&result);
        emit_value(&result, &text, output)?;
        Ok(true)
    }

    pub fn invoke_join(&self, room: &str, invite: &str, output: OutputFormat) -> Result<bool> {
        let room_id = parse_room(room)?;
        let invite =
            InviteProof::new(invite.to_owned()).map_err(|_| eyre!("room invite is invalid"))?;
        let mut client = self.client_with_invite(1, Some(invite.clone()))?;
        let observation = client.observe(&room_id)?;
        let result = client.invoke_payload(
            &observation,
            &CommandPayload::RedeemInvite { invite },
            random_command_id("room-join")?,
        )?;
        let text = action_result_text(&result);
        emit_value(&result, &text, output)?;
        Ok(true)
    }

    fn client(
        &self,
        epoch: u64,
    ) -> Result<PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>> {
        self.client_with_invite(epoch, None)
    }

    fn client_with_invite(
        &self,
        epoch: u64,
        join_invite: Option<InviteProof>,
    ) -> Result<PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>> {
        let store = ProtectedProfileStore::open_default()?;
        let profile = if let Some(label) = &self.profile {
            store.load_device(label)?
        } else {
            let mut profiles = store.list_devices()?;
            if profiles.len() != 1 {
                return Err(eyre!(
                    "select one protected device with --profile; exactly one is required for implicit selection"
                ));
            }
            profiles.remove(0)
        };
        let mut transport = HttpDeviceTransport::new(&self.endpoint, epoch, store)?;
        if let Some(invite) = join_invite {
            transport = transport.with_join_invite(invite);
        }
        Ok(PlayerDeviceClient::new(profile, transport)?)
    }
}

fn parse_room(value: &str) -> Result<RoomId> {
    RoomId::new(value.to_owned()).map_err(|_| eyre!("room identifier is invalid"))
}

fn random_command_id(prefix: &str) -> Result<CommandId> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|_| eyre!("operating-system randomness is unavailable"))?;
    let suffix = random.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    });
    CommandId::new(format!("{prefix}-{suffix}"))
        .map_err(|_| eyre!("command identifier could not be constructed"))
}

fn emit_observation(observation: &DeviceObservation, output: OutputFormat) -> Result<()> {
    let projection = &observation.projection;
    let text = format!(
        "room: {}\nepoch: {}\nrevision: {}\nviewer: {}\nphase: {}\nactions:\n{}",
        projection.room_id.as_str(),
        projection.session_epoch,
        projection.current_revision,
        projection.principal_id.as_str(),
        format_args!("{:?}", projection.payload.phase),
        actions_text(&observation.actions)
    );
    emit_value(observation, &text, output)
}

fn actions_text(actions: &[AdvertisedAction]) -> String {
    if actions.is_empty() {
        return "  (none)".to_owned();
    }
    actions
        .iter()
        .map(|action| format!("  {} — {}", action.id, action.label))
        .collect::<Vec<_>>()
        .join("\n")
}

fn action_result_text(result: &DeviceActionResult) -> String {
    match result {
        DeviceActionResult::Committed {
            command_id,
            revision,
        } => format!("committed {} at revision {revision}", command_id.as_str()),
        DeviceActionResult::Denied { command_id, code } => {
            format!("denied {}: {code}", command_id.as_str())
        }
    }
}

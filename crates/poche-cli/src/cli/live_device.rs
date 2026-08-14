// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use eyre::{Result, eyre};
use poche_player_client::{
    AdvertisedAction, AdvertisedActionPolicy, DeviceActionResult, DeviceClientError,
    DeviceObservation, HttpDeviceTransport, PlayerDeviceClient, PolicyScope, ProtectedProfileStore,
};
use poche_protocol::{CommandId, CommandPayload, InviteProof, RoomId};
use serde::Serialize;

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

    pub fn invoke_matching_payload(
        &self,
        room: &str,
        epoch: u64,
        command_prefix: &str,
        description: &str,
        output: OutputFormat,
        matches: impl Fn(&CommandPayload) -> bool,
    ) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(epoch)?;
        let observation = client.observe(&room_id)?;
        let mut candidates = observation
            .actions
            .iter()
            .filter(|action| matches(&action.payload));
        let action = candidates
            .next()
            .ok_or_else(|| eyre!("{description} is not currently advertised"))?;
        if candidates.next().is_some() {
            return Err(eyre!(
                "{description} is ambiguous in the current advertised action set"
            ));
        }
        let result = client.invoke(&observation, &action.id, random_command_id(command_prefix)?)?;
        let text = action_result_text(&result);
        emit_value(&result, &text, output)?;
        Ok(true)
    }

    pub fn chat_tail(&self, room: &str, limit: usize, output: OutputFormat) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client(1)?;
        let observation = client.observe(&room_id)?;
        let start = observation.chat_tail.len().saturating_sub(limit);
        let entries = &observation.chat_tail[start..];
        let text = if entries.is_empty() {
            "No chat messages.".to_owned()
        } else {
            entries
                .iter()
                .map(|entry| {
                    format!(
                        "{} · {}: {}",
                        entry.revision,
                        entry.principal_id.as_str(),
                        entry.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        emit_value(entries, &text, output)?;
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

    pub fn run_agent(
        &self,
        profile: &str,
        room: &str,
        policy: AdvertisedActionPolicy,
        output: OutputFormat,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<bool> {
        let room_id = parse_room(room)?;
        let mut client = self.client_for_profile(1, None, Some(profile))?;
        let mut observation = client.observe(&room_id)?;
        let initial_revision = observation.projection.current_revision;
        let mut committed_actions = 0_u64;
        let mut denied_actions = 0_u64;
        while !cancelled() {
            if let Some(action_id) = policy
                .select(&observation, PolicyScope::PlayerGameActions)
                .map(|action| action.id.clone())
            {
                match client.invoke(&observation, &action_id, random_command_id("agent-action")?)? {
                    DeviceActionResult::Committed { .. } => {
                        committed_actions = committed_actions.saturating_add(1);
                    }
                    DeviceActionResult::Denied { .. } => {
                        denied_actions = denied_actions.saturating_add(1);
                    }
                }
                observation = client.observe(&room_id)?;
                continue;
            }
            match client.wait(&room_id, observation.projection.current_revision) {
                Ok(next) => observation = next,
                Err(DeviceClientError::NoProgress | DeviceClientError::StaleRevision) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let summary = AgentRunSummary {
            schema: "poche.cli.agent-run.v1",
            profile: client.profile().label.clone(),
            room_id: room_id.as_str().to_owned(),
            initial_revision,
            final_revision: observation.projection.current_revision,
            committed_actions,
            denied_actions,
            stop_reason: "cancelled",
        };
        let text = format!(
            "agent {} stopped at revision {} ({} committed, {} denied)",
            summary.profile,
            summary.final_revision,
            summary.committed_actions,
            summary.denied_actions
        );
        emit_value(&summary, &text, output)?;
        Ok(true)
    }

    pub(crate) fn existing_room_client(
        &self,
        room: &str,
    ) -> Result<(
        PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>,
        RoomId,
    )> {
        let room_id = parse_room(room)?;
        Ok((self.client(1)?, room_id))
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
        self.client_for_profile(epoch, join_invite, None)
    }

    pub(crate) fn client_for_profile(
        &self,
        epoch: u64,
        join_invite: Option<InviteProof>,
        profile_override: Option<&str>,
    ) -> Result<PlayerDeviceClient<HttpDeviceTransport<ProtectedProfileStore>>> {
        let store = ProtectedProfileStore::open_default()?;
        let profile = if let Some(label) = profile_override.or(self.profile.as_deref()) {
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

#[derive(Serialize)]
struct AgentRunSummary {
    schema: &'static str,
    profile: String,
    room_id: String,
    initial_revision: u64,
    final_revision: u64,
    committed_actions: u64,
    denied_actions: u64,
    stop_reason: &'static str,
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

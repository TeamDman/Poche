// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Browser-device authentication, idempotency, and retained SSE delivery.

use std::{
    collections::{BTreeMap, VecDeque},
    io::Write as _,
    sync::{Arc, Mutex},
};

use ed25519_dalek::{Signature, VerifyingKey};
use flate2::{Compression, write::GzEncoder};
use poche_protocol::DeviceCustodyWire;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

const COMMAND_DOMAIN: &str = "poche.gateway-command.v1";
const MAX_DEVICE_COUNT: usize = 16;
const MAX_COMMAND_ID_BYTES: usize = 96;
const MAX_CHAT_BYTES: usize = 256;
const RETAINED_EVENT_LIMIT: usize = 128;

/// Browser-to-gateway semantic action. Raw command strings never cross this
/// ingress boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayAction {
    Pause,
    Unpause,
    Chat { text: String },
    GrantSpectator,
    RevokeSpectator,
}

impl GatewayAction {
    fn signing_parts(&self) -> (&'static str, &str) {
        match self {
            Self::Pause => ("pause", ""),
            Self::Unpause => ("unpause", ""),
            Self::Chat { text } => ("chat", text),
            Self::GrantSpectator => ("grant_spectator", ""),
            Self::RevokeSpectator => ("revoke_spectator", ""),
        }
    }

    fn validate(&self) -> Result<(), GatewayError> {
        if let Self::Chat { text } = self
            && (text.is_empty() || text.len() > MAX_CHAT_BYTES || text.contains('\0'))
        {
            return Err(GatewayError::InvalidAction);
        }
        Ok(())
    }
}

/// Public registration material for a non-extractable browser-local key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayDeviceRegistration {
    pub schema_version: u16,
    pub player_id: String,
    pub device_id: String,
    pub signing_public_key: String,
    pub custody: DeviceCustodyWire,
}

/// Signed, bounded command submitted over ordinary HTTP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedGatewayCommand {
    pub schema_version: u16,
    pub player_id: String,
    pub device_id: String,
    pub command_id: String,
    pub sequence: u64,
    pub action: GatewayAction,
    pub signature: String,
}

/// Inspectable response proving whether a semantic effect was newly attempted
/// or suppressed as an exact retry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GatewayCommandReceipt {
    pub command_id: String,
    pub event_id: u64,
    pub semantic_status: String,
    pub duplicate: bool,
    pub server_latency_micros: u64,
}

/// One exact-recipient SSE data value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GatewayProjectionEvent {
    pub event_id: u64,
    pub recipient_device_id: String,
    pub html: String,
    pub semantic_status: String,
}

/// Public device inventory. Secret material is never represented here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GatewayDeviceView {
    pub player_id: String,
    pub device_id: String,
    pub custody: DeviceCustodyWire,
    pub active: bool,
    pub role: String,
}

/// Measured gateway evidence displayed to both the operator and player.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GatewayMetrics {
    pub accepted_commands: u64,
    pub duplicate_retries: u64,
    pub rejected_commands: u64,
    pub sse_connections: u64,
    pub sse_reconnects: u64,
    pub last_command_bytes: usize,
    pub last_projection_raw_bytes: usize,
    pub last_projection_gzip_bytes: usize,
    pub last_server_latency_micros: u64,
    pub last_action_kind: Option<String>,
    pub last_recipient_device_id: Option<String>,
}

#[derive(Clone)]
pub struct GatewayLab {
    inner: Arc<Mutex<GatewayInner>>,
    sender: broadcast::Sender<GatewayProjectionEvent>,
}

struct GatewayInner {
    devices: BTreeMap<String, DeviceRecord>,
    receipts: BTreeMap<String, StoredReceipt>,
    events: VecDeque<GatewayProjectionEvent>,
    next_event_id: u64,
    metrics: GatewayMetrics,
}

struct DeviceRecord {
    player_id: String,
    key: VerifyingKey,
    custody: DeviceCustodyWire,
    active: bool,
    role: String,
    last_sequence: u64,
}

#[derive(Clone)]
struct StoredReceipt {
    signing_bytes: Vec<u8>,
    receipt: GatewayCommandReceipt,
}

pub enum CommandDecision {
    Applied(GatewayCommandReceipt),
    Duplicate(GatewayCommandReceipt),
}

impl GatewayLab {
    /// Construct the lab with a separately identified, co-located native
    /// fixture. Its key is not used to sign browser commands or increase player
    /// voting weight.
    pub fn new() -> Result<Self, GatewayError> {
        let (sender, _) = broadcast::channel(RETAINED_EVENT_LIMIT);
        let native_key = VerifyingKey::from_bytes(
            &ed25519_dalek::SigningKey::from_bytes(&[0x71; 32])
                .verifying_key()
                .to_bytes(),
        )
        .map_err(|_| GatewayError::InvalidKey)?;
        let native_id = hex(&native_key.to_bytes());
        let mut devices = BTreeMap::new();
        devices.insert(
            native_id,
            DeviceRecord {
                player_id: "alice".to_owned(),
                key: native_key,
                custody: DeviceCustodyWire::NativeLocal,
                active: true,
                role: "co-located native fixture".to_owned(),
                last_sequence: 0,
            },
        );
        Ok(Self {
            inner: Arc::new(Mutex::new(GatewayInner {
                devices,
                receipts: BTreeMap::new(),
                events: VecDeque::new(),
                next_event_id: 1,
                metrics: GatewayMetrics::default(),
            })),
            sender,
        })
    }

    pub fn register_browser(
        &self,
        registration: &GatewayDeviceRegistration,
        initial_html: String,
    ) -> Result<GatewayDeviceView, GatewayError> {
        if registration.schema_version != 1
            || registration.player_id != "alice"
            || registration.custody != DeviceCustodyWire::BrowserLocal
            || registration.device_id != registration.signing_public_key
        {
            return Err(GatewayError::InvalidRegistration);
        }
        let key = VerifyingKey::from_bytes(&decode_hex::<32>(&registration.signing_public_key)?)
            .map_err(|_| GatewayError::InvalidKey)?;
        let mut inner = self.inner.lock().map_err(|_| GatewayError::Poisoned)?;
        if inner.devices.len() >= MAX_DEVICE_COUNT
            && !inner.devices.contains_key(&registration.device_id)
        {
            return Err(GatewayError::DeviceLimit);
        }
        if let Some(existing) = inner.devices.get(&registration.device_id) {
            if !existing.active {
                return Err(GatewayError::Revoked);
            }
            if existing.player_id != registration.player_id
                || existing.custody != registration.custody
                || existing.key != key
            {
                return Err(GatewayError::InvalidRegistration);
            }
        } else {
            inner.devices.insert(
                registration.device_id.clone(),
                DeviceRecord {
                    player_id: registration.player_id.clone(),
                    key,
                    custody: registration.custody,
                    active: true,
                    role: "browser-local session device".to_owned(),
                    last_sequence: 0,
                },
            );
        }
        let device = device_view(
            registration.device_id.clone(),
            inner
                .devices
                .get(&registration.device_id)
                .ok_or(GatewayError::UnknownDevice)?,
        );
        self.publish_locked(
            &mut inner,
            &registration.device_id,
            initial_html,
            "registered browser-local device".to_owned(),
        )?;
        Ok(device)
    }

    /// Verify, deduplicate, execute exactly once, and retain recipient updates.
    pub fn apply_command(
        &self,
        command: &SignedGatewayCommand,
        execute: impl FnOnce(&GatewayAction) -> (String, String),
    ) -> Result<CommandDecision, GatewayError> {
        let started = std::time::Instant::now();
        let signing_bytes = command_signing_bytes(command)?;
        let mut inner = self.inner.lock().map_err(|_| GatewayError::Poisoned)?;

        if let Some(stored) = inner.receipts.get(&command.command_id).cloned() {
            if stored.signing_bytes != signing_bytes {
                inner.metrics.rejected_commands = inner.metrics.rejected_commands.saturating_add(1);
                return Err(GatewayError::ConflictingRetry);
            }
            inner.metrics.duplicate_retries = inner.metrics.duplicate_retries.saturating_add(1);
            let mut receipt = stored.receipt;
            receipt.duplicate = true;
            return Ok(CommandDecision::Duplicate(receipt));
        }

        let device = inner
            .devices
            .get(&command.device_id)
            .ok_or(GatewayError::UnknownDevice)?;
        if command.schema_version != 1
            || command.player_id != device.player_id
            || !device.active
            || command.sequence <= device.last_sequence
        {
            let error = if device.active {
                GatewayError::StaleSequence
            } else {
                GatewayError::Revoked
            };
            inner.metrics.rejected_commands = inner.metrics.rejected_commands.saturating_add(1);
            return Err(error);
        }
        let signature = Signature::from_bytes(&decode_hex::<64>(&command.signature)?);
        if device
            .key
            .verify_strict(&signing_bytes, &signature)
            .is_err()
        {
            inner.metrics.rejected_commands = inner.metrics.rejected_commands.saturating_add(1);
            return Err(GatewayError::BadSignature);
        }
        inner
            .devices
            .get_mut(&command.device_id)
            .ok_or(GatewayError::UnknownDevice)?
            .last_sequence = command.sequence;

        let (semantic_status, html) = execute(&command.action);
        let elapsed = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
        let event_id = self.publish_locked(
            &mut inner,
            &command.device_id,
            html,
            semantic_status.clone(),
        )?;
        let receipt = GatewayCommandReceipt {
            command_id: command.command_id.clone(),
            event_id,
            semantic_status,
            duplicate: false,
            server_latency_micros: elapsed,
        };
        inner.receipts.insert(
            command.command_id.clone(),
            StoredReceipt {
                signing_bytes: signing_bytes.clone(),
                receipt: receipt.clone(),
            },
        );
        let (kind, _) = command.action.signing_parts();
        inner.metrics.accepted_commands = inner.metrics.accepted_commands.saturating_add(1);
        inner.metrics.last_command_bytes = signing_bytes.len() + 64;
        inner.metrics.last_server_latency_micros = elapsed;
        inner.metrics.last_action_kind = Some(kind.to_owned());
        inner.metrics.last_recipient_device_id = Some(command.device_id.clone());
        Ok(CommandDecision::Applied(receipt))
    }

    /// A separately authorized native fixture revokes only the selected device.
    pub fn revoke_from_native(
        &self,
        target: &str,
        html: &str,
    ) -> Result<GatewayDeviceView, GatewayError> {
        let mut inner = self.inner.lock().map_err(|_| GatewayError::Poisoned)?;
        let record = inner
            .devices
            .get_mut(target)
            .ok_or(GatewayError::UnknownDevice)?;
        if record.custody != DeviceCustodyWire::BrowserLocal {
            return Err(GatewayError::CannotRevokeFixture);
        }
        record.active = false;
        let view = device_view(target.to_owned(), record);
        let remaining = inner
            .devices
            .iter()
            .filter(|(_, candidate)| candidate.player_id == "alice" && candidate.active)
            .map(|(device_id, _)| device_id.clone())
            .collect::<Vec<_>>();
        for device_id in remaining {
            self.publish_locked(
                &mut inner,
                &device_id,
                html.to_owned(),
                format!("native device revoked browser device {target}"),
            )?;
        }
        Ok(view)
    }

    pub fn subscribe(
        &self,
        device_id: &str,
        after: u64,
    ) -> Result<
        (
            Vec<GatewayProjectionEvent>,
            broadcast::Receiver<GatewayProjectionEvent>,
        ),
        GatewayError,
    > {
        let mut inner = self.inner.lock().map_err(|_| GatewayError::Poisoned)?;
        let device = inner
            .devices
            .get(device_id)
            .ok_or(GatewayError::UnknownDevice)?;
        if !device.active {
            return Err(GatewayError::Revoked);
        }
        inner.metrics.sse_connections = inner.metrics.sse_connections.saturating_add(1);
        if after > 0 {
            inner.metrics.sse_reconnects = inner.metrics.sse_reconnects.saturating_add(1);
        }
        let history = inner
            .events
            .iter()
            .filter(|event| event.recipient_device_id == device_id && event.event_id > after)
            .cloned()
            .collect();
        Ok((history, self.sender.subscribe()))
    }

    pub fn devices(&self) -> Result<Vec<GatewayDeviceView>, GatewayError> {
        let inner = self.inner.lock().map_err(|_| GatewayError::Poisoned)?;
        Ok(inner
            .devices
            .iter()
            .map(|(id, record)| device_view(id.clone(), record))
            .collect())
    }

    pub fn metrics(&self) -> Result<GatewayMetrics, GatewayError> {
        self.inner
            .lock()
            .map_err(|_| GatewayError::Poisoned)
            .map(|inner| inner.metrics.clone())
    }

    fn publish_locked(
        &self,
        inner: &mut GatewayInner,
        recipient: &str,
        html: String,
        semantic_status: String,
    ) -> Result<u64, GatewayError> {
        let event_id = inner.next_event_id;
        inner.next_event_id = inner.next_event_id.saturating_add(1);
        let event = GatewayProjectionEvent {
            event_id,
            recipient_device_id: recipient.to_owned(),
            html,
            semantic_status,
        };
        let encoded = serde_json::to_vec(&event).map_err(|_| GatewayError::Encoding)?;
        inner.metrics.last_projection_raw_bytes = encoded.len();
        inner.metrics.last_projection_gzip_bytes = gzip_len(&encoded)?;
        inner.events.push_back(event.clone());
        if inner.events.len() > RETAINED_EVENT_LIMIT {
            inner.events.pop_front();
        }
        let _ = self.sender.send(event);
        Ok(event_id)
    }
}

pub fn command_signing_bytes(command: &SignedGatewayCommand) -> Result<Vec<u8>, GatewayError> {
    if command.schema_version != 1
        || command.player_id != "alice"
        || !is_lower_hex(&command.device_id, 64)
        || command.command_id.is_empty()
        || command.command_id.len() > MAX_COMMAND_ID_BYTES
        || !command
            .command_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || command.sequence == 0
    {
        return Err(GatewayError::MalformedCommand);
    }
    command.action.validate()?;
    let (kind, text) = command.action.signing_parts();
    let schema_version = command.schema_version.to_string();
    let sequence = command.sequence.to_string();
    let parts = [
        COMMAND_DOMAIN.as_bytes(),
        schema_version.as_bytes(),
        command.player_id.as_bytes(),
        command.device_id.as_bytes(),
        command.command_id.as_bytes(),
        sequence.as_bytes(),
        kind.as_bytes(),
        text.as_bytes(),
    ];
    let mut bytes = Vec::new();
    for part in parts {
        let length = u32::try_from(part.len()).map_err(|_| GatewayError::MalformedCommand)?;
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(part);
    }
    Ok(bytes)
}

fn device_view(device_id: String, record: &DeviceRecord) -> GatewayDeviceView {
    GatewayDeviceView {
        player_id: record.player_id.clone(),
        device_id,
        custody: record.custody,
        active: record.active,
        role: record.role.clone(),
    }
}

fn gzip_len(bytes: &[u8]) -> Result<usize, GatewayError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|_| GatewayError::Encoding)?;
    encoder
        .finish()
        .map(|compressed| compressed.len())
        .map_err(|_| GatewayError::Encoding)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], GatewayError> {
    if !is_lower_hex(value, N * 2) {
        return Err(GatewayError::InvalidKey);
    }
    let mut bytes = [0_u8; N];
    for (target, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| GatewayError::InvalidKey)?;
        *target = u8::from_str_radix(pair, 16).map_err(|_| GatewayError::InvalidKey)?;
    }
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayError {
    InvalidRegistration,
    InvalidKey,
    DeviceLimit,
    UnknownDevice,
    Revoked,
    CannotRevokeFixture,
    MalformedCommand,
    InvalidAction,
    StaleSequence,
    BadSignature,
    ConflictingRetry,
    Encoding,
    Poisoned,
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRegistration => "registration is not the browser-local Alice lab profile",
            Self::InvalidKey => "device public key is invalid",
            Self::DeviceLimit => "device registration limit reached",
            Self::UnknownDevice => "device is not registered",
            Self::Revoked => "device has been revoked",
            Self::CannotRevokeFixture => "the co-located native fixture cannot revoke itself",
            Self::MalformedCommand => "signed command fields are malformed",
            Self::InvalidAction => "typed action is invalid or over its bound",
            Self::StaleSequence => "device sequence is stale",
            Self::BadSignature => "device signature is invalid",
            Self::ConflictingRetry => "command ID was reused with different signed bytes",
            Self::Encoding => "gateway evidence encoding failed",
            Self::Poisoned => "gateway state lock is poisoned",
        })
    }
}

impl std::error::Error for GatewayError {}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer as _, SigningKey};

    use super::*;

    fn signed_command(key: &SigningKey, sequence: u64, command_id: &str) -> SignedGatewayCommand {
        let mut command = SignedGatewayCommand {
            schema_version: 1,
            player_id: "alice".to_owned(),
            device_id: hex(&key.verifying_key().to_bytes()),
            command_id: command_id.to_owned(),
            sequence,
            action: GatewayAction::Pause,
            signature: "00".repeat(64),
        };
        command.signature = hex(&key
            .sign(&command_signing_bytes(&command).expect("signing bytes"))
            .to_bytes());
        command
    }

    fn registered_lab() -> (GatewayLab, SigningKey, String) {
        let lab = GatewayLab::new().expect("gateway lab");
        let key = SigningKey::from_bytes(&[0x25; 32]);
        let device_id = hex(&key.verifying_key().to_bytes());
        lab.register_browser(
            &GatewayDeviceRegistration {
                schema_version: 1,
                player_id: "alice".to_owned(),
                device_id: device_id.clone(),
                signing_public_key: device_id.clone(),
                custody: DeviceCustodyWire::BrowserLocal,
            },
            "<main>initial exact projection</main>".to_owned(),
        )
        .expect("register browser");
        (lab, key, device_id)
    }

    #[test]
    fn signature_retry_and_conflict_are_distinct() {
        let (lab, key, _) = registered_lab();
        let command = signed_command(&key, 1, "same-command");
        let first = lab
            .apply_command(&command, |_| {
                ("applied".to_owned(), "<main>paused</main>".to_owned())
            })
            .expect("first command");
        assert!(matches!(first, CommandDecision::Applied(_)));
        let retry = lab
            .apply_command(&command, |_| panic!("retry must not execute"))
            .expect("exact retry");
        assert!(matches!(retry, CommandDecision::Duplicate(_)));

        let mut conflict = command;
        conflict.action = GatewayAction::Unpause;
        assert!(matches!(
            lab.apply_command(&conflict, |_| unreachable!()),
            Err(GatewayError::ConflictingRetry)
        ));
    }

    #[test]
    fn reconnect_replays_only_later_exact_recipient_events() {
        let (lab, key, device_id) = registered_lab();
        let initial = lab.subscribe(&device_id, 0).expect("initial stream").0;
        assert_eq!(initial.len(), 1);
        let after = initial[0].event_id;
        let command = signed_command(&key, 1, "pause-once");
        lab.apply_command(&command, |_| {
            ("applied".to_owned(), "<main>paused once</main>".to_owned())
        })
        .expect("pause");
        let replay = lab.subscribe(&device_id, after).expect("reconnect").0;
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].semantic_status, "applied");
        assert_eq!(lab.metrics().unwrap().accepted_commands, 1);
    }

    #[test]
    fn native_fixture_revokes_only_the_browser_device() {
        let (lab, key, device_id) = registered_lab();
        lab.revoke_from_native(&device_id, "<main>revoked</main>")
            .expect("revoke browser");
        let command = signed_command(&key, 1, "after-revoke");
        assert!(matches!(
            lab.apply_command(&command, |_| unreachable!()),
            Err(GatewayError::Revoked)
        ));
        let devices = lab.devices().expect("inventory");
        assert_eq!(devices.iter().filter(|device| device.active).count(), 1);
        assert_eq!(
            devices
                .iter()
                .filter(|device| device.player_id == "alice")
                .count(),
            2
        );
        assert!(matches!(
            lab.register_browser(
                &GatewayDeviceRegistration {
                    schema_version: 1,
                    player_id: "alice".to_owned(),
                    device_id: device_id.clone(),
                    signing_public_key: device_id,
                    custody: DeviceCustodyWire::BrowserLocal,
                },
                "<main>must not reactivate</main>".to_owned(),
            ),
            Err(GatewayError::Revoked)
        ));
    }

    #[test]
    fn oversized_chat_and_bad_signature_fail_closed() {
        let (lab, key, _) = registered_lab();
        let mut oversized = signed_command(&key, 1, "oversized");
        oversized.action = GatewayAction::Chat {
            text: "x".repeat(MAX_CHAT_BYTES + 1),
        };
        assert!(matches!(
            command_signing_bytes(&oversized),
            Err(GatewayError::InvalidAction)
        ));

        let mut bad = signed_command(&key, 1, "bad-signature");
        bad.signature = "00".repeat(64);
        assert!(matches!(
            lab.apply_command(&bad, |_| unreachable!()),
            Err(GatewayError::BadSignature)
        ));
    }
}

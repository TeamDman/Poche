// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A narrow, owned bridge between Bevy's frame schedule and the renderer-free
//! Poche `SpacetimeDB` client.

use bevy::prelude::*;
use poche_spacetimedb_client::{
    ClientConfig, ClientEvent, ClientSnapshot, PocheClient, RoomCapability,
};
use std::sync::{Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum BridgeIntent {
    Connect {
        config: ClientConfig,
    },
    Disconnect,
    Create {
        display_name: String,
    },
    Join {
        display_name: String,
        join_code: String,
    },
    TakeSeat {
        room_id: String,
        seat: u8,
    },
    ReleaseSeat {
        room_id: String,
    },
    SetCardPose {
        room_id: String,
        card_id: String,
        sequence: u64,
        position_mm: [i32; 3],
        rotation_mdeg: [i32; 3],
    },
    Bid {
        room_id: String,
        tricks: u8,
    },
    PlayCard {
        room_id: String,
        card_id: String,
    },
    Leave {
        room_id: String,
    },
    Stop,
}

#[derive(Message, Clone, Debug)]
pub enum BridgeNotice {
    Connected {
        identity: String,
    },
    RoomCreated(RoomCapability),
    Snapshot(ClientSnapshot),
    Command {
        operation: &'static str,
        elapsed: Duration,
        result: Result<(), String>,
    },
    Disconnected(Option<String>),
    Error(String),
}

#[derive(Resource, Clone, Debug, Default)]
pub struct BridgeModel {
    pub connected: bool,
    pub snapshot: ClientSnapshot,
    pub last_error: Option<String>,
    pub last_command_latency: Option<Duration>,
}

#[derive(Debug)]
enum WorkerEvent {
    Notice(BridgeNotice),
}

#[derive(Resource)]
pub struct BridgeHandle {
    request_tx: mpsc::SyncSender<BridgeIntent>,
    event_rx: Mutex<mpsc::Receiver<WorkerEvent>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Default for BridgeHandle {
    fn default() -> Self {
        Self::start().expect("Poche SpacetimeDB bridge worker should start")
    }
}

impl BridgeHandle {
    /// Starts the renderer-neutral client worker.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot create the worker thread.
    pub fn start() -> Result<Self, String> {
        let (request_tx, request_rx) = mpsc::sync_channel(256);
        let (event_tx, event_rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("poche-spacetimedb-client".into())
            .spawn(move || worker_loop(request_rx, event_tx))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            request_tx,
            event_rx: Mutex::new(event_rx),
            worker: Mutex::new(Some(worker)),
        })
    }

    /// Queues a player or lifecycle intent without blocking Bevy's frame loop.
    ///
    /// # Errors
    ///
    /// Returns an error when the bounded command queue is full or disconnected.
    pub fn send(&self, intent: BridgeIntent) -> Result<(), String> {
        self.request_tx
            .try_send(intent)
            .map_err(|error| format!("client command queue unavailable: {error}"))
    }
}

impl Drop for BridgeHandle {
    fn drop(&mut self) {
        let _ = self.request_tx.try_send(BridgeIntent::Stop);
        if let Some(worker) = self.worker.lock().expect("worker mutex poisoned").take() {
            let _ = worker.join();
        }
    }
}

pub struct PocheSpacetimePlugin;

impl Plugin for PocheSpacetimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BridgeHandle>()
            .init_resource::<BridgeModel>()
            .add_message::<BridgeNotice>()
            .add_systems(Update, pump_bridge);
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are passed by value.
fn pump_bridge(
    bridge: Res<BridgeHandle>,
    mut model: ResMut<BridgeModel>,
    mut notices: MessageWriter<BridgeNotice>,
) {
    let receiver = bridge
        .event_rx
        .lock()
        .expect("event receiver mutex poisoned");
    for event in receiver.try_iter() {
        let WorkerEvent::Notice(notice) = event;
        match &notice {
            BridgeNotice::Connected { .. } => model.connected = true,
            BridgeNotice::Snapshot(snapshot) => model.snapshot.clone_from(snapshot),
            BridgeNotice::Command {
                elapsed, result, ..
            } => {
                model.last_command_latency = Some(*elapsed);
                if let Err(error) = result {
                    model.last_error = Some(error.clone());
                }
            }
            BridgeNotice::Disconnected(reason) => {
                model.connected = false;
                model.last_error.clone_from(reason);
            }
            BridgeNotice::Error(error) => model.last_error = Some(error.clone()),
            BridgeNotice::RoomCreated(_) => {}
        }
        notices.write(notice);
    }
}

#[allow(clippy::needless_pass_by_value)] // The worker thread owns both channel endpoints.
#[allow(
    clippy::too_many_lines,
    reason = "the single bridge owner serializes the complete small command vocabulary in one auditable loop"
)]
fn worker_loop(requests: mpsc::Receiver<BridgeIntent>, events: mpsc::Sender<WorkerEvent>) {
    let mut client: Option<PocheClient> = None;
    loop {
        match requests.recv_timeout(Duration::from_millis(4)) {
            Ok(BridgeIntent::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Ok(BridgeIntent::Connect { config }) => {
                client = None;
                send(&events, BridgeNotice::Snapshot(ClientSnapshot::default()));
                match PocheClient::connect(config, Duration::from_secs(8)) {
                    Ok(connected) => {
                        let snapshot = connected.snapshot();
                        let identity = snapshot.identity.clone().unwrap_or_default();
                        send(&events, BridgeNotice::Connected { identity });
                        send(&events, BridgeNotice::Snapshot(snapshot));
                        client = Some(connected);
                    }
                    Err(error) => send(&events, BridgeNotice::Error(error.to_string())),
                }
            }
            Ok(BridgeIntent::Disconnect) => {
                client = None;
                send(&events, BridgeNotice::Snapshot(ClientSnapshot::default()));
                send(&events, BridgeNotice::Disconnected(None));
            }
            Ok(intent) => {
                let Some(connected) = client.as_ref() else {
                    send(
                        &events,
                        BridgeNotice::Error("connect before sending room actions".into()),
                    );
                    continue;
                };
                let result = match intent {
                    BridgeIntent::Create { display_name } => {
                        match connected.create_room(display_name) {
                            Ok(capability) => {
                                send(&events, BridgeNotice::RoomCreated(capability));
                                Ok(0)
                            }
                            Err(error) => Err(error),
                        }
                    }
                    BridgeIntent::Join {
                        display_name,
                        join_code,
                    } => connected.join_room(join_code, display_name),
                    BridgeIntent::TakeSeat { room_id, seat } => connected.take_seat(room_id, seat),
                    BridgeIntent::ReleaseSeat { room_id } => connected.release_seat(room_id),
                    BridgeIntent::SetCardPose {
                        room_id,
                        card_id,
                        sequence,
                        position_mm,
                        rotation_mdeg,
                    } => connected.set_card_pose(
                        room_id,
                        card_id,
                        sequence,
                        position_mm,
                        rotation_mdeg,
                    ),
                    BridgeIntent::Bid { room_id, tricks } => connected.bid(room_id, tricks),
                    BridgeIntent::PlayCard { room_id, card_id } => {
                        connected.play_card(room_id, card_id)
                    }
                    BridgeIntent::Leave { room_id } => connected.leave_room(room_id),
                    BridgeIntent::Connect { .. }
                    | BridgeIntent::Disconnect
                    | BridgeIntent::Stop => unreachable!(),
                };
                if let Err(error) = result {
                    send(&events, BridgeNotice::Error(error.to_string()));
                }
            }
        }

        if let Some(connected) = client.as_ref() {
            let mut changed = false;
            for event in connected.drain_events() {
                match event {
                    ClientEvent::ModelChanged | ClientEvent::Ready => changed = true,
                    ClientEvent::CommandFinished {
                        operation,
                        elapsed,
                        result,
                        ..
                    } => send(
                        &events,
                        BridgeNotice::Command {
                            operation,
                            elapsed,
                            result,
                        },
                    ),
                    ClientEvent::Disconnected { reason } => {
                        send(&events, BridgeNotice::Disconnected(reason));
                    }
                    ClientEvent::Error(error) => send(&events, BridgeNotice::Error(error)),
                    ClientEvent::Connected { .. } => {}
                }
            }
            if changed {
                send(&events, BridgeNotice::Snapshot(connected.snapshot()));
            }
        }
    }
}

fn send(events: &mpsc::Sender<WorkerEvent>, notice: BridgeNotice) {
    let _ = events.send(WorkerEvent::Notice(notice));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_starts_and_stops_without_a_connection() {
        let bridge = BridgeHandle::start().expect("bridge starts");
        drop(bridge);
    }
}

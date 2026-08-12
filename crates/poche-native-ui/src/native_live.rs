use std::{
    sync::{Mutex, mpsc},
    thread,
};

use bevy::prelude::Resource;
use poche_player_client::{
    DeviceActionResult, DeviceObservation, DeviceTransport, PlayerDeviceClient,
};
use poche_protocol::{CommandId, RoomId};

use crate::{CommittedPresentation, advertised_action_for_play};

enum NativeWorkerCommand {
    Invoke {
        observation: DeviceObservation,
        action_id: String,
        command_id: CommandId,
    },
}

enum NativeWorkerEvent {
    Updated {
        result: DeviceActionResult,
        observation: Box<DeviceObservation>,
    },
    Failed(String),
}

/// Nonblocking bridge between the Bevy update loop and one ordinary certified
/// player-device client. The worker owns the configured transport; this is not
/// local-instance control and grants no authority over another process.
#[derive(Resource)]
pub struct NativeLiveDevice {
    observation: DeviceObservation,
    commands: mpsc::SyncSender<NativeWorkerCommand>,
    events: Mutex<mpsc::Receiver<NativeWorkerEvent>>,
    next_command: u64,
    last_result: Option<DeviceActionResult>,
}

impl NativeLiveDevice {
    /// Connect the graphical adapter as its own device and obtain its first
    /// exact-recipient observation before opening a window.
    ///
    /// # Errors
    ///
    /// Returns a stable client error string when the initial observation fails
    /// or a worker thread cannot be started.
    pub fn connect<T>(mut client: PlayerDeviceClient<T>, room_id: RoomId) -> Result<Self, String>
    where
        T: DeviceTransport + Send + 'static,
    {
        let observation = client
            .observe(&room_id)
            .map_err(|error| error.to_string())?;
        let (command_tx, command_rx) = mpsc::sync_channel(8);
        let (event_tx, event_rx) = mpsc::sync_channel(8);
        thread::Builder::new()
            .name("poche-native-device".to_owned())
            .spawn(move || {
                while let Ok(command) = command_rx.recv() {
                    match command {
                        NativeWorkerCommand::Invoke {
                            observation,
                            action_id,
                            command_id,
                        } => {
                            let result = client.invoke(&observation, &action_id, command_id);
                            match result {
                                Ok(result) => match client.observe(&room_id) {
                                    Ok(observation) => {
                                        if event_tx
                                            .send(NativeWorkerEvent::Updated {
                                                result,
                                                observation: Box::new(observation),
                                            })
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        if event_tx
                                            .send(NativeWorkerEvent::Failed(error.to_string()))
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                },
                                Err(error) => {
                                    if event_tx
                                        .send(NativeWorkerEvent::Failed(error.to_string()))
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .map_err(|_| "native device worker could not start".to_owned())?;
        Ok(Self {
            observation,
            commands: command_tx,
            events: Mutex::new(event_rx),
            next_command: 0,
            last_result: None,
        })
    }

    /// Borrow the exact projection currently rendered by the native adapter.
    #[must_use]
    pub const fn observation(&self) -> &DeviceObservation {
        &self.observation
    }

    /// Queue one spatially accepted play through the opaque action advertised
    /// beside the exact rendered projection. This never blocks Bevy on I/O.
    ///
    /// # Errors
    ///
    /// Rejects missing advertised action identity, a full worker queue, or a
    /// disconnected worker.
    pub fn submit_play(&mut self, committed: &CommittedPresentation) -> Result<(), String> {
        let action = advertised_action_for_play(&self.observation, committed)
            .ok_or_else(|| "spatial play has no exact advertised action".to_owned())?;
        let command_id = CommandId::new(format!("native-device-{}", self.next_command))
            .map_err(|_| "native device command ID is invalid".to_owned())?;
        self.next_command = self.next_command.saturating_add(1);
        self.commands
            .try_send(NativeWorkerCommand::Invoke {
                observation: self.observation.clone(),
                action_id: action.id.clone(),
                command_id,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "native device worker is busy".to_owned(),
                mpsc::TrySendError::Disconnected(_) => {
                    "native device worker is unavailable".to_owned()
                }
            })
    }

    /// Apply all currently available worker results without blocking.
    ///
    /// # Errors
    ///
    /// Returns a stable worker/client error received since the last poll.
    pub fn poll(&mut self) -> Result<bool, String> {
        let mut changed = false;
        let receiver = self
            .events
            .lock()
            .map_err(|_| "native device event queue is unavailable".to_owned())?;
        loop {
            match receiver.try_recv() {
                Ok(NativeWorkerEvent::Updated {
                    result,
                    observation,
                }) => {
                    self.last_result = Some(result);
                    self.observation = *observation;
                    changed = true;
                }
                Ok(NativeWorkerEvent::Failed(error)) => return Err(error),
                Err(mpsc::TryRecvError::Empty) => return Ok(changed),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("native device worker disconnected".to_owned());
                }
            }
        }
    }

    /// Borrow the latest ordinary authority result, if one has completed.
    #[must_use]
    pub const fn last_result(&self) -> Option<&DeviceActionResult> {
        self.last_result.as_ref()
    }
}

//! Test-only barriers carry expected public metadata, never commands or private
//! hands. Every actual observation and action still crosses the device service.
use super::*;
use crate::input_probe::rendered::{self, DragPose, DragTicket, ExpectedPlay};
use poche_player_client::DeviceObservation;
use poche_protocol::{CommandPayload, GameActionWire, PublicGameEventWire, PublicGamePhase};

pub(super) enum Progress {
    Wait,
    Invoke(String),
    Done,
}

enum Stage {
    Playing,
    PeerDenied,
    Dragging(ExpectedPlay, DragTicket),
    PeerAccepted(DragPose),
    NextRound,
    Agreement(Vec<u8>),
}

pub(super) struct Probe {
    coordination: PathBuf,
    evidence: PathBuf,
    stage: Stage,
    actor: bool,
    round: u16,
    entered: Option<Instant>,
}

impl Probe {
    pub(super) fn new(coordination: PathBuf, evidence: PathBuf) -> Self {
        Self {
            coordination,
            evidence,
            stage: Stage::Playing,
            actor: false,
            round: 0,
            entered: None,
        }
    }

    pub(super) fn description(&self) -> String {
        let stage = match self.stage {
            Stage::Playing => "both rendered bids",
            Stage::PeerDenied => "hidden peer pose after denied play",
            Stage::Dragging(ExpectedPlay::Denied, _) => "out-of-turn rendered drag",
            Stage::Dragging(ExpectedPlay::Accepted, _) => "legal rendered drag",
            Stage::PeerAccepted(_) => "public peer pose after legal play",
            Stage::NextRound => "scoring and next deal",
            Stage::Agreement(_) => "independent public-history agreement",
        };
        format!("waiting for {stage}")
    }

    fn set(&mut self, stage: Stage) {
        self.stage = stage;
        self.entered = Some(Instant::now());
    }

    pub(super) fn advance(&mut self, world: &mut World) -> Result<Progress, String> {
        if self.entered.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(110) {
            return Err(format!("trick stage timed out: {}", self.description()));
        }
        let view = world.resource::<NativeLiveDevice>().observation();
        match &self.stage {
            Stage::Playing => {
                let Some(game) = view
                    .projection
                    .payload
                    .public_game_state
                    .as_ref()
                    .filter(|game| game.phase == PublicGamePhase::Playing)
                else {
                    return Ok(Progress::Wait);
                };
                if game.hand_size != 1 || !game.current_trick.is_empty() {
                    return Err("rendered trick requires the first one-card round".into());
                }
                self.round = game.round_index;
                self.actor = view.actions.iter().any(|action| {
                    matches!(
                        action.payload,
                        CommandPayload::GameAction {
                            action: GameActionWire::Play { .. }
                        }
                    )
                });
                if self.actor {
                    self.set(Stage::PeerDenied);
                } else {
                    let ticket = rendered::begin_in_world(
                        world,
                        ExpectedPlay::Denied,
                        &self.evidence.join("denied"),
                    )?;
                    self.set(Stage::Dragging(ExpectedPlay::Denied, ticket));
                }
            }
            Stage::PeerDenied => {
                let Some(expected) = read_pose(&self.coordination.join("trick-denied.json"))?
                else {
                    return Ok(Progress::Wait);
                };
                if expected.public_face.is_some() {
                    return Err("denied metadata contains a face".into());
                }
                if view.projection.current_revision > expected.revision {
                    return Err("logical revision advanced during the denied-play barrier".into());
                }
                if view.projection.current_revision != expected.revision
                    || !hidden_matches(view, &expected)?
                {
                    return Ok(Progress::Wait);
                }
                let ticket = rendered::begin_in_world(
                    world,
                    ExpectedPlay::Accepted,
                    &self.evidence.join("accepted"),
                )?;
                self.set(Stage::Dragging(ExpectedPlay::Accepted, ticket));
            }
            Stage::Dragging(expected, ticket) => {
                let outcome = ticket.lock().map_err(|_| "drag result unavailable")?.take();
                let Some(pose) = outcome else {
                    return Ok(Progress::Wait);
                };
                let pose = pose?;
                let expected = *expected;
                let name = if expected == ExpectedPlay::Denied {
                    "trick-denied.json"
                } else {
                    "trick-accepted.json"
                };
                publish(
                    &self.coordination.join(name),
                    &serde_json::to_vec(&pose).map_err(|_| "pose encoding failed")?,
                )?;
                self.set(if expected == ExpectedPlay::Denied {
                    Stage::PeerAccepted(pose)
                } else {
                    Stage::NextRound
                });
            }
            Stage::PeerAccepted(denied) => {
                let Some(expected) = read_pose(&self.coordination.join("trick-accepted.json"))?
                else {
                    return Ok(Progress::Wait);
                };
                if expected.public_face.is_none() {
                    return Err("accepted metadata lacks its now-public face".into());
                }
                if !public_matches(view, &expected)? {
                    return Ok(Progress::Wait);
                }
                let own = view
                    .physical_hands
                    .iter()
                    .find(|card| card.id == denied.card_id)
                    .ok_or("denied card disappeared from the own hand")?;
                if own.pose.as_ref().is_none_or(|pose| {
                    pose.position_mm != denied.position_mm
                        || pose.rotation_millidegrees != denied.rotation_millidegrees
                }) {
                    return Err("denied card snapped back while waiting for legal turn".into());
                }
                let face = own.face.ok_or("own card lost its authorized face")?;
                let action = view.actions.iter().find(|action| matches!(action.payload,
                    CommandPayload::GameAction { action: GameActionWire::Play { card } } if card == face))
                    .ok_or("previously denied card has no legal action on this turn")?.id.clone();
                self.set(Stage::NextRound);
                return Ok(Progress::Invoke(action));
            }
            Stage::NextRound => {
                let payload = &view.projection.payload;
                let Some(game) = payload
                    .public_game_state
                    .as_ref()
                    .filter(|game| game.round_index == self.round + 1)
                else {
                    return Ok(Progress::Wait);
                };
                if game.phase != PublicGamePhase::Bidding
                    || payload
                        .own_hand
                        .as_ref()
                        .is_none_or(|hand| hand.cards.len() != 2)
                {
                    return Ok(Progress::Wait);
                }
                let plays = payload
                    .public_history
                    .iter()
                    .filter(|event| {
                        matches!(
                            event,
                            PublicGameEventWire::PlayerAction {
                                action: GameActionWire::Play { .. },
                                ..
                            }
                        )
                    })
                    .count();
                let scored = payload
                    .public_history
                    .iter()
                    .filter(|event| matches!(event, PublicGameEventWire::RoundScored { .. }))
                    .count();
                if plays != 2 || scored != 1 || !payload.granted_hands.is_empty() {
                    return Err(
                        "completed trick has wrong action history or leaked hand grants".into(),
                    );
                }
                // Full public values, not private projections or local UI status.
                let public = serde_json::to_vec(&(game, &payload.public_history))
                    .map_err(|_| "public evidence encoding failed")?;
                let name = if self.actor {
                    "trick-actor-result.json"
                } else {
                    "trick-follower-result.json"
                };
                publish(&self.coordination.join(name), &public)?;
                self.set(Stage::Agreement(public));
            }
            Stage::Agreement(public) => {
                let name = if self.actor {
                    "trick-follower-result.json"
                } else {
                    "trick-actor-result.json"
                };
                let Some(peer) = read_optional(&self.coordination.join(name))? else {
                    return Ok(Progress::Wait);
                };
                if peer != *public {
                    return Err("independent devices disagree on scored public history".into());
                }
                eprintln!(
                    "poche rendered probe: complete trick, private-safe movement and public history agreement verified"
                );
                return Ok(Progress::Done);
            }
        }
        Ok(Progress::Wait)
    }
}

fn hidden_matches(view: &DeviceObservation, expected: &DragPose) -> Result<bool, String> {
    if view
        .physical_public
        .iter()
        .any(|card| card.id == expected.card_id)
    {
        return Err("out-of-turn drag published a private card".into());
    }
    let Some(card) = view
        .physical_hands
        .iter()
        .find(|card| card.id == expected.card_id)
    else {
        return Ok(false);
    };
    if card.face.is_some() {
        return Err("out-of-turn drag exposed a peer's face".into());
    }
    Ok(card.pose.as_ref().is_some_and(|pose| {
        pose.position_mm == expected.position_mm
            && pose.rotation_millidegrees == expected.rotation_millidegrees
    }))
}

fn public_matches(view: &DeviceObservation, expected: &DragPose) -> Result<bool, String> {
    let Some(card) = view
        .physical_public
        .iter()
        .find(|card| card.id == expected.card_id)
    else {
        return Ok(false);
    };
    if Some(card.face) != expected.public_face
        || view
            .physical_hands
            .iter()
            .any(|card| card.id == expected.card_id)
    {
        return Err("accepted play has inconsistent public identity".into());
    }
    Ok(view.projection.current_revision >= expected.revision
        && card.pose.position_mm == expected.position_mm
        && card.pose.rotation_millidegrees == expected.rotation_millidegrees)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("test barrier read failed".into()),
    }
}
fn read_pose(path: &Path) -> Result<Option<DragPose>, String> {
    read_optional(path)?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| "invalid pose evidence".into()))
        .transpose()
}
fn publish(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // Single writer per marker in a fresh parent-owned directory. Rename means
    // a reader sees the entire expected observation or no marker yet.
    let temporary = path.with_extension("pending");
    std::fs::write(&temporary, bytes).map_err(|_| "test barrier write failed")?;
    std::fs::rename(temporary, path).map_err(|_| "test barrier publication failed".into())
}

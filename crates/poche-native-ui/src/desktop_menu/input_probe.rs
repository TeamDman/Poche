//! Windowless menu acceptance. Supplies pointer/keyboard input, never an
//! Interaction, text-value mutation, menu request or privileged game command.
//! Clipboard storage is explicitly isolated from the user's OS clipboard.
use super::*;
mod trick;
use crate::{
    AcceptanceOptions, LaunchClock, NativeLiveDevice, NativeRenderMode, NativeUiLaunchOptions,
};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    window::PrimaryWindow,
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct IsolatedClipboard(Arc<Mutex<String>>);

impl IsolatedClipboard {
    pub fn text(&self) -> String {
        self.0.lock().expect("isolated clipboard").clone()
    }
}

pub enum MenuScenario {
    Create,
    /// Publish the copied invitation to the test coordinator only after the
    /// creator has taken seat 0 and readied through the rendered controls.
    CreateAndDeal {
        invitation_ready: Box<dyn FnOnce(&str) -> Result<(), String> + Send + Sync>,
    },
    Join {
        invitation: String,
    },
    JoinAndDeal {
        invitation: String,
    },
    CreateAndTrick {
        invitation_ready: Box<dyn FnOnce(&str) -> Result<(), String> + Send + Sync>,
        coordination: PathBuf,
    },
    JoinAndTrick {
        invitation: String,
        coordination: PathBuf,
    },
    /// The supplied worker must return this redacted error on each request.
    ConnectionFailure {
        invitation: String,
        error: &'static str,
    },
}

/// Keep the connection worker alive along with its room/service owners after
/// the test renderer exits. Dropping just the renderer must not kill the room.
pub struct MenuConnection {
    pub live: NativeLiveDevice,
    pub worker: DesktopConnectionWorker,
}

/// Exercise real rendered menu controls through the connection worker. A
/// successful scenario returns its live device for independent peer checks.
///
/// # Errors
/// Returns incorrect input/validation/connection/copy, missing capture or
/// bounded timeout errors. Requires a fresh artifact directory; screenshots
/// can contain room invitations and are local private developer evidence.
pub fn run(
    worker: DesktopConnectionWorker,
    validator: InvitationValidator,
    name: &str,
    scenario: MenuScenario,
    clipboard: IsolatedClipboard,
    directory: &Path,
) -> Result<Option<MenuConnection>, String> {
    std::fs::create_dir(directory).map_err(|_| "a fresh menu evidence directory is required")?;
    let (scenario, trick) = match scenario {
        MenuScenario::CreateAndTrick {
            invitation_ready,
            coordination,
        } => (
            MenuScenario::CreateAndDeal { invitation_ready },
            Some(coordination),
        ),
        MenuScenario::JoinAndTrick {
            invitation,
            coordination,
        } => (MenuScenario::JoinAndDeal { invitation }, Some(coordination)),
        scenario => (scenario, None),
    };
    let timeout = Duration::from_secs(if trick.is_some() { 360 } else { 120 });
    let mut steps = VecDeque::from([
        Step::Click(Target::Create, 0),
        Step::Status("Enter a name first.".to_owned()),
        Step::Click(Target::Name, 0),
    ]);
    steps.extend(name.chars().map(Step::Type));
    steps.push_back(Step::Field(Field::Name, name.to_owned()));
    match scenario {
        MenuScenario::Create | MenuScenario::CreateAndDeal { .. } => {
            steps.push_back(Step::Capture("menu.png", false));
            steps.push_back(Step::Click(Target::Create, 0));
            steps.push_back(Step::Live);
            steps.push_back(Step::Click(Target::Copy, 0));
            steps.push_back(Step::Copied);
            steps.push_back(Step::Capture("lobby.png", false));
            if let MenuScenario::CreateAndDeal { invitation_ready } = scenario {
                steps.extend([
                    Step::Invoke("room-take-seat-0".into()),
                    Step::OwnSeat(0),
                    Step::Invoke("room-ready".into()),
                    Step::OwnReady,
                    Step::PublishInvitation(invitation_ready),
                    Step::Invoke("countdown-arm".into()),
                    Step::Dealt,
                    Step::Capture("dealt.png", false),
                ]);
            }
        }
        MenuScenario::Join { ref invitation }
        | MenuScenario::JoinAndDeal { ref invitation }
        | MenuScenario::ConnectionFailure { ref invitation, .. } => {
            // Non-invitation bytes must not enter the editor or start a request.
            steps.extend([
                Step::Clipboard("unrelated isolated clipboard data".to_owned()),
                Step::Click(Target::Paste, 0),
                Step::Status("No valid lobby invitation found in the clipboard.".to_owned()),
                Step::Field(Field::Invitation, String::new()),
                Step::Idle,
                Step::Clipboard(invitation.clone()),
                Step::Click(Target::Paste, 0),
                Step::Status("Invitation pasted. Choose Join when ready.".to_owned()),
                Step::Field(Field::Invitation, invitation.clone()),
                Step::Idle,
                Step::Capture("menu.png", false),
                Step::Click(Target::Join, 0),
            ]);
            if let MenuScenario::ConnectionFailure { error, .. } = scenario {
                steps.extend([
                    Step::Status(error.to_owned()),
                    Step::Idle,
                    Step::Capture("join-error.png", false),
                    Step::Click(Target::Create, 0),
                    Step::Status(error.to_owned()),
                    Step::Idle,
                    Step::Capture("create-error.png", false),
                ]);
            } else {
                steps.push_back(Step::Live);
                steps.push_back(Step::Capture("lobby.png", false));
                if matches!(scenario, MenuScenario::JoinAndDeal { .. }) {
                    steps.extend([
                        Step::Invoke("room-take-seat-1".into()),
                        Step::OwnSeat(1),
                        Step::Invoke("room-ready".into()),
                        Step::OwnReady,
                        Step::Dealt,
                        Step::Capture("dealt.png", false),
                    ]);
                }
            }
        }
        MenuScenario::CreateAndTrick { .. } | MenuScenario::JoinAndTrick { .. } => unreachable!(),
    }
    if let Some(coordination) = trick {
        steps.extend([
            Step::Bid,
            Step::Trick(trick::Probe::new(coordination, directory.to_owned())),
            Step::Capture("after-trick.png", false),
        ]);
    }
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    let driver = Driver {
        steps,
        directory: directory.to_owned(),
        clipboard: clipboard.clone(),
        result: shared,
        frame: 0,
        ready_at: 60,
        started: Instant::now(),
        timeout,
        error: None,
        captures: Vec::new(),
    };
    crate::run_configured(
        NativeUiLaunchOptions {
            render_mode: NativeRenderMode::WindowlessImage,
            external_tracing: bevy::log::tracing::dispatcher::has_been_set(),
            ..default()
        },
        None,
        Some((worker, validator)),
        move |app| {
            // Bevy 0.19 focused-keyboard dispatch requires a primary endpoint.
            // Winit is disabled by WindowlessImage: this creates NO OS window.
            // Rendering and picking still use the real image camera and layout.
            app.world_mut().spawn((
                Window {
                    visible: false,
                    ..default()
                },
                PrimaryWindow,
            ));
            app.insert_resource(InvitationClipboard::Isolated(clipboard.0))
                .insert_resource(driver)
                .add_systems(Last, (drive, crate::input_probe::rendered::drive).chain());
        },
    )?;
    result
        .lock()
        .map_err(|_| "menu result unavailable")?
        .take()
        .ok_or_else(|| "menu stopped before finishing input acceptance".to_owned())?
}

#[derive(Clone, Debug)]
enum Target {
    Name,
    Create,
    Paste,
    Join,
    Copy,
    LiveAction(String),
}

enum Step {
    Click(Target, u8),
    Type(char),
    Status(String),
    Field(Field, String),
    Clipboard(String),
    Idle,
    Live,
    Copied,
    Invoke(String),
    Committed(String, Option<poche_player_client::DeviceActionResult>),
    OwnSeat(u8),
    OwnReady,
    PublishInvitation(Box<dyn FnOnce(&str) -> Result<(), String> + Send + Sync>),
    Dealt,
    Bid,
    Trick(trick::Probe),
    Capture(&'static str, bool),
}

impl Step {
    // Keep failure context useful without printing names, invitations or faces.
    fn description(&self) -> String {
        match self {
            Self::Click(target, stage) => format!("click {target:?}, stage {stage}"),
            Self::Invoke(action) => format!("waiting for advertised {action}"),
            Self::Committed(action, _) => format!("waiting for committed {action}"),
            Self::Dealt => "waiting for own dealt hand".to_owned(),
            Self::Bid => "waiting for an advertised bid".to_owned(),
            Self::Trick(probe) => probe.description(),
            Self::OwnSeat(_) | Self::OwnReady => "checking own lobby membership".to_owned(),
            Self::Live => "waiting for connection".to_owned(),
            Self::Capture(_, _) => "saving screenshot".to_owned(),
            _ => "checking menu input".to_owned(),
        }
    }
}

#[derive(Resource)]
struct Driver {
    steps: VecDeque<Step>,
    directory: PathBuf,
    clipboard: IsolatedClipboard,
    result: Arc<Mutex<Option<Result<Option<MenuConnection>, String>>>>,
    frame: u64,
    ready_at: u64,
    started: Instant,
    timeout: Duration,
    error: Option<String>,
    captures: Vec<PathBuf>,
}

fn drive(world: &mut World) {
    let Some(mut driver) = world.remove_resource::<Driver>() else {
        return;
    };
    driver.frame += 1;
    if driver.frame < driver.ready_at {
        world.insert_resource(driver);
        return;
    }
    let result = if driver.started.elapsed() > driver.timeout && driver.error.is_none() {
        Err(format!(
            "rendered input timed out: {}",
            driver
                .steps
                .front()
                .map_or_else(|| "finishing".to_owned(), Step::description)
        ))
    } else {
        driver.advance(world)
    };
    if let Err(error) = result {
        if driver.error.is_some() {
            *driver.result.lock().expect("menu result") = Some(Err(driver.error.unwrap()));
            world.write_message(AppExit::Success);
            return;
        }
        driver.error = Some(error);
        driver.steps = VecDeque::from([Step::Capture("failure.png", false)]);
    }
    if driver.steps.is_empty() {
        let result = if let Some(error) = driver.error {
            Err(error)
        } else {
            driver
                .captures
                .iter()
                .try_for_each(|path| {
                    let pixels = image::open(path)
                        .map_err(|_| "menu capture missing")?
                        .to_rgba8();
                    if pixels.width() != crate::AUTOMATION_RENDER_WIDTH
                        || pixels.height() != crate::AUTOMATION_RENDER_HEIGHT
                        || pixels.pixels().all(|pixel| pixel == pixels.get_pixel(0, 0))
                    {
                        return Err("menu capture blank or wrong size");
                    }
                    Ok(())
                })
                .map_err(str::to_owned)
                .map(|()| {
                    world
                        .remove_resource::<NativeLiveDevice>()
                        .map(|live| MenuConnection {
                            live,
                            worker: world
                                .remove_resource::<DesktopConnectionWorker>()
                                .expect("menu owns connection worker"),
                        })
                })
        };
        *driver.result.lock().expect("menu result") = Some(result);
        world.write_message(AppExit::Success);
    } else {
        world.insert_resource(driver);
    }
}

impl Driver {
    fn advance(&mut self, world: &mut World) -> Result<(), String> {
        let Some(step) = self.steps.pop_front() else {
            return Ok(());
        };
        match step {
            Step::Click(target, stage) => {
                let entity = find_target(world, &target)
                    .ok_or_else(|| format!("missing {target:?} control"))?;
                let transform = world
                    .get::<UiGlobalTransform>(entity)
                    .ok_or("UI geometry missing")?;
                let position = transform.translation;
                let camera = world
                    .get::<ComputedUiTargetCamera>(entity)
                    .and_then(ComputedUiTargetCamera::get)
                    .ok_or("UI camera missing")?;
                let render_target = world
                    .get::<bevy::camera::RenderTarget>(camera)
                    .and_then(|target| target.normalize(None))
                    .ok_or("image UI target missing")?;
                let action = match stage {
                    0 => PointerAction::Move { delta: Vec2::ZERO },
                    1 => PointerAction::Press(PointerButton::Primary),
                    _ => PointerAction::Release(PointerButton::Primary),
                };
                world.write_message(PointerInput::new(
                    PointerId::Mouse,
                    Location {
                        target: render_target,
                        position,
                    },
                    action,
                ));
                if stage < 2 {
                    self.steps.push_front(Step::Click(target, stage + 1));
                }
                self.ready_at = self.frame + 3;
            }
            Step::Type(character) => {
                let window = world
                    .query_filtered::<Entity, With<PrimaryWindow>>()
                    .single(world)
                    .map_err(|_| "headless input endpoint missing")?;
                for state in [ButtonState::Pressed, ButtonState::Released] {
                    world.write_message(KeyboardInput {
                        key_code: KeyCode::Unidentified(
                            bevy::input::keyboard::NativeKeyCode::Unidentified,
                        ),
                        logical_key: Key::Character(character.to_string().into()),
                        state,
                        text: Some(character.to_string().into()),
                        repeat: false,
                        window,
                    });
                }
                self.ready_at = self.frame + 3;
            }
            Step::Status(expected) => {
                let status = world.resource::<DesktopMenuStatus>();
                if status.busy {
                    self.steps.push_front(Step::Status(expected));
                } else if status.message != expected {
                    return Err(format!("menu status mismatch: {}", status.message));
                }
            }
            Step::Field(field, expected) => {
                let mut fields =
                    world.query::<(&Field, &EditableText, &bevy::text::TextLayoutInfo)>();
                let found = fields.iter(world).find(|(kind, _, _)| **kind == field);
                let actual = found.map(|(_, text, _)| text.value().into_iter().collect::<String>());
                if actual.as_deref() != Some(&expected) {
                    return Err("keyboard/clipboard did not populate expected field".to_owned());
                }
                if !expected.is_empty()
                    && found.is_none_or(|(_, _, layout)| layout.glyphs.is_empty())
                {
                    return Err("populated editor has no rendered glyphs".to_owned());
                }
            }
            Step::Clipboard(text) => {
                *self.clipboard.0.lock().expect("isolated clipboard") = text;
            }
            Step::Idle => {
                if world.resource::<DesktopMenuStatus>().busy
                    || world.contains_resource::<NativeLiveDevice>()
                    || world.contains_resource::<crate::NativeController>()
                {
                    return Err(
                        "validation/prefill unexpectedly connected or created a fixture scene"
                            .to_owned(),
                    );
                }
            }
            Step::Live => {
                if !world.contains_resource::<NativeLiveDevice>() {
                    let status = world.resource::<DesktopMenuStatus>();
                    if !status.busy {
                        return Err(format!("menu connection failed: {}", status.message));
                    }
                    self.steps.push_front(Step::Live);
                } else {
                    if !world.contains_resource::<crate::NativeController>()
                        || world
                            .query_filtered::<Entity, With<DesktopMenuRoot>>()
                            .iter(world)
                            .next()
                            .is_some()
                    {
                        return Err(
                            "successful connection did not replace the menu with a live table"
                                .to_owned(),
                        );
                    }
                    self.ready_at = self.frame + 30;
                }
            }
            Step::Copied => {
                let live = world.resource::<NativeLiveDevice>();
                if live.room_invitation() != Some(self.clipboard.text().as_str()) {
                    return Err("Copy button did not export the live room invitation".to_owned());
                }
            }
            Step::Invoke(action) => {
                let live = world.resource::<NativeLiveDevice>();
                if !live
                    .observation()
                    .actions
                    .iter()
                    .any(|candidate| candidate.id == action)
                {
                    self.steps.push_front(Step::Invoke(action));
                } else {
                    eprintln!(
                        "poche rendered probe: clicking {action} at revision {}",
                        live.observation().projection.current_revision
                    );
                    self.steps
                        .push_front(Step::Committed(action.clone(), live.last_result().cloned()));
                    self.steps
                        .push_front(Step::Click(Target::LiveAction(action), 0));
                }
            }
            Step::Committed(action, previous) => {
                let live = world.resource::<NativeLiveDevice>();
                let result = live.last_result();
                if result == previous.as_ref() {
                    self.steps.push_front(Step::Committed(action, previous));
                } else if !matches!(
                    result,
                    Some(poche_player_client::DeviceActionResult::Committed { .. })
                ) {
                    return Err(format!(
                        "rendered {action} did not receive a committed result"
                    ));
                }
            }
            Step::OwnSeat(seat) => {
                let view = world.resource::<NativeLiveDevice>().observation();
                if !view.projection.payload.members.iter().any(|member| {
                    member.principal_id == view.projection.principal_id && member.seat == Some(seat)
                }) {
                    return Err("rendered seat action did not assign this player's seat".to_owned());
                }
            }
            Step::OwnReady => {
                let view = world.resource::<NativeLiveDevice>().observation();
                if !view.projection.payload.members.iter().any(|member| {
                    member.principal_id == view.projection.principal_id && member.ready
                }) {
                    return Err("rendered Ready did not ready this player".to_owned());
                }
            }
            Step::PublishInvitation(publish) => {
                let copied = self.clipboard.text();
                let live = world.resource::<NativeLiveDevice>();
                if live.room_invitation() != Some(copied.as_str()) {
                    return Err("copied invitation changed before publication".to_owned());
                }
                publish(&copied)?;
            }
            Step::Dealt => {
                let view = world.resource::<NativeLiveDevice>().observation();
                if view
                    .projection
                    .payload
                    .own_hand
                    .as_ref()
                    .is_none_or(|hand| hand.cards.is_empty())
                {
                    self.steps.push_front(Step::Dealt);
                } else {
                    if view.projection.payload.phase != poche_protocol::RoomPhase::Running
                        || !view.projection.payload.granted_hands.is_empty()
                    {
                        return Err(
                            "dealt view has wrong phase or unexpected hand grants".to_owned()
                        );
                    }
                    // Let the real projection/mesh and hand-camera update settle.
                    self.ready_at = self.frame + 30;
                }
            }
            Step::Bid => {
                let action = world
                    .resource::<NativeLiveDevice>()
                    .observation()
                    .actions
                    .iter()
                    .find(|action| {
                        matches!(
                            action.payload,
                            poche_protocol::CommandPayload::GameAction {
                                action: poche_protocol::GameActionWire::Bid { .. }
                            }
                        )
                    })
                    .map(|action| action.id.clone());
                if let Some(action) = action {
                    self.steps.push_front(Step::Invoke(action));
                } else {
                    self.steps.push_front(Step::Bid);
                }
            }
            Step::Trick(mut probe) => match probe.advance(world)? {
                trick::Progress::Wait => self.steps.push_front(Step::Trick(probe)),
                trick::Progress::Invoke(action) => {
                    self.steps.push_front(Step::Trick(probe));
                    self.steps.push_front(Step::Invoke(action));
                }
                trick::Progress::Done => {}
            },
            Step::Capture(name, pending) => {
                if !pending {
                    let path = self.directory.join(name);
                    self.captures.push(path.clone());
                    world.resource_mut::<AcceptanceOptions>().screenshot = Some(path);
                    let mut clock = world.resource_mut::<LaunchClock>();
                    clock.screenshot_requested = false;
                    clock.screenshot_completed = false;
                    self.steps.push_front(Step::Capture(name, true));
                    self.ready_at = self.frame + 3;
                } else if !world.resource::<LaunchClock>().screenshot_completed {
                    if self.started.elapsed() > self.timeout + Duration::from_secs(15) {
                        return Err("menu screenshot timed out".to_owned());
                    }
                    self.steps.push_front(Step::Capture(name, true));
                }
            }
        }
        Ok(())
    }
}

fn find_target(world: &mut World, target: &Target) -> Option<Entity> {
    if let Target::LiveAction(id) = target {
        return world
            .query::<(Entity, &LiveAction)>()
            .iter(world)
            .find(|(_, action)| action.0 == *id)
            .map(|(entity, _)| entity);
    }
    if matches!(target, Target::Name) {
        return world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| **field == Field::Name)
            .map(|(entity, _)| entity);
    }
    if matches!(target, Target::Copy) {
        return world
            .query_filtered::<Entity, With<CopyInvitation>>()
            .iter(world)
            .next();
    }
    world
        .query::<(Entity, &Action)>()
        .iter(world)
        .find(|(_, action)| {
            matches!(
                (target, **action),
                (Target::Create, Action::Create)
                    | (Target::Paste, Action::Paste)
                    | (Target::Join, Action::Join)
            )
        })
        .map(|(entity, _)| entity)
}

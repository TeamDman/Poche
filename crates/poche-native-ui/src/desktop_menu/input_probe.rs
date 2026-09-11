//! Windowless menu acceptance. Supplies pointer/keyboard input, never an
//! Interaction, text-value mutation, menu request or privileged game command.
//! Clipboard storage is explicitly isolated from the user's OS clipboard.
use super::*;
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
    Join {
        invitation: String,
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
    let mut steps = VecDeque::from([
        Step::Click(Target::Create, 0),
        Step::Status("Enter a name first.".to_owned()),
        Step::Click(Target::Name, 0),
    ]);
    steps.extend(name.chars().map(Step::Type));
    steps.push_back(Step::Field(Field::Name, name.to_owned()));
    match scenario {
        MenuScenario::Create => {
            steps.push_back(Step::Capture("menu.png", false));
            steps.push_back(Step::Click(Target::Create, 0));
            steps.push_back(Step::Live);
            steps.push_back(Step::Click(Target::Copy, 0));
            steps.push_back(Step::Copied);
            steps.push_back(Step::Capture("lobby.png", false));
        }
        MenuScenario::Join { ref invitation }
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
            }
        }
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
                .add_systems(Last, drive);
        },
    )?;
    result
        .lock()
        .map_err(|_| "menu result unavailable")?
        .take()
        .ok_or_else(|| "menu stopped before finishing input acceptance".to_owned())?
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Name,
    Create,
    Paste,
    Join,
    Copy,
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
    Capture(&'static str, bool),
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
    let result = if driver.started.elapsed() > Duration::from_secs(120) && driver.error.is_none() {
        Err("menu input acceptance timed out".to_owned())
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
                let entity = find_target(world, target)
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
                    if self.started.elapsed() > Duration::from_secs(135) {
                        return Err("menu screenshot timed out".to_owned());
                    }
                    self.steps.push_front(Step::Capture(name, true));
                }
            }
        }
        Ok(())
    }
}

fn find_target(world: &mut World, target: Target) -> Option<Entity> {
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

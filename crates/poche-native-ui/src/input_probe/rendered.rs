//! GPU/image-target acceptance using the normal app and only pointer input.
//! This is a local test driver, not a new player control or capture protocol.
use super::*;
use bevy::picking::hover::HoverMap;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedPlay {
    Denied,
    Accepted,
}

/// The authenticated device is returned for independent recipient checks.
pub struct RenderedDragResult {
    pub live: NativeLiveDevice,
    pub card_id: String,
    pub position_mm: [i32; 3],
    pub rotation_millidegrees: [i32; 3],
}

/// Render and drag an owned card from the hand inset to PLAY. The caller must
/// supply a fresh artifact directory; captures contain this viewer's hand.
///
/// # Errors
/// Fails on incorrect picking, lost gesture, pose disagreement, wrong play
/// outcome, unreadable/blank capture, or a bounded timeout. Opens no window.
pub fn drag_to_play(
    live: NativeLiveDevice,
    expected: ExpectedPlay,
    directory: &Path,
) -> Result<RenderedDragResult, String> {
    std::fs::create_dir(directory).map_err(|_| "a fresh capture directory is required")?;
    let controller = native_controller_from_observation(live.observation())?;
    let face = if expected == ExpectedPlay::Accepted {
        live.observation()
            .actions
            .iter()
            .find_map(|action| match action.payload {
                poche_protocol::CommandPayload::GameAction {
                    action: poche_protocol::GameActionWire::Play { card },
                } => CardFace::new(card),
                _ => None,
            })
            .ok_or("accepted probe needs an advertised legal card")?
    } else {
        if live.observation().actions.iter().any(|action| {
            matches!(
                action.payload,
                poche_protocol::CommandPayload::GameAction {
                    action: poche_protocol::GameActionWire::Play { .. }
                }
            )
        }) {
            return Err("denied probe requires an out-of-turn player".to_owned());
        }
        controller
            .first_owned_face()
            .ok_or("denied probe needs an owned card")?
    };
    let id = live
        .observation()
        .physical_hands
        .iter()
        .find(|card| card.face == Some(face.code()))
        .ok_or("missing physical identity")?
        .id
        .clone();
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    let probe = Probe {
        directory: directory.to_owned(),
        expected,
        face,
        id,
        revision: live.observation().projection.current_revision,
        initial_game: live
            .observation()
            .projection
            .payload
            .public_game_state
            .clone(),
        stage: Stage::Warmup,
        frame: 0,
        entered: 0,
        started: None,
        card: None,
        play: None,
        target: None,
        start: Vec2::ZERO,
        destination: Vec2::ZERO,
        pointer: Vec2::ZERO,
        position: [0; 3],
        rotation: [0; 3],
        initial_rotation: [0; 3],
        error: None,
        result: shared,
    };
    run_configured(
        NativeUiLaunchOptions {
            render_mode: NativeRenderMode::WindowlessImage,
            external_tracing: bevy::log::tracing::dispatcher::has_been_set(),
            // Screenshot persistence uses the same acceptance_driver as ordinary
            // local captures; this driver only chooses the moment and fresh path.
            ..default()
        },
        Some(live),
        None,
        move |app| {
            app.insert_resource(probe).add_systems(Last, drive);
        },
    )?;
    let outcome = result
        .lock()
        .map_err(|_| "probe result unavailable")?
        .take()
        .ok_or("renderer stopped before completing its input probe")??;
    for name in ["before.png", "held.png", "after.png"] {
        let image = image::open(directory.join(name))
            .map_err(|_| "capture was not saved")?
            .to_rgba8();
        if image.width() != AUTOMATION_RENDER_WIDTH
            || image.height() != AUTOMATION_RENDER_HEIGHT
            || image.pixels().all(|pixel| pixel == image.get_pixel(0, 0))
        {
            return Err("capture has no meaningful render content".to_owned());
        }
    }
    Ok(outcome)
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Warmup,
    BeforeCapture,
    Hover,
    Press,
    StartDrag,
    Move(u8),
    AwaitPose,
    HeldCapture,
    Release,
    AwaitOutcome,
    AfterCapture,
    FailureCapture,
}

#[derive(Resource)]
struct Probe {
    directory: PathBuf,
    expected: ExpectedPlay,
    face: CardFace,
    id: String,
    revision: u64,
    initial_game: Option<poche_protocol::GamePublicStateWire>,
    stage: Stage,
    frame: u64,
    entered: u64,
    started: Option<Instant>,
    card: Option<Entity>,
    play: Option<Entity>,
    target: Option<bevy::camera::NormalizedRenderTarget>,
    start: Vec2,
    destination: Vec2,
    pointer: Vec2,
    position: [i32; 3],
    rotation: [i32; 3],
    initial_rotation: [i32; 3],
    error: Option<String>,
    result: Arc<Mutex<Option<Result<RenderedDragResult, String>>>>,
}

fn drive(world: &mut World) {
    let Some(mut probe) = world.remove_resource::<Probe>() else {
        return;
    };
    probe.frame += 1;
    let elapsed = probe.started.get_or_insert_with(Instant::now).elapsed();
    let result = if elapsed > Duration::from_secs(90) && probe.error.is_none() {
        Err(format!("rendered input timed out in {:?}", probe.stage))
    } else {
        probe.advance(world)
    };
    if let Err(error) = result {
        probe.error = Some(error);
        probe.capture(world, "failure.png");
        probe.change(Stage::FailureCapture);
    }
    let terminal = matches!(probe.stage, Stage::AfterCapture | Stage::FailureCapture)
        && (world.resource::<LaunchClock>().screenshot_completed
            || probe.frame - probe.entered > 600);
    if terminal {
        let outcome = if let Some(error) = probe.error.take() {
            Err(error)
        } else if !world.resource::<LaunchClock>().screenshot_completed {
            Err("final screenshot timed out".to_owned())
        } else {
            Ok(RenderedDragResult {
                live: world
                    .remove_resource::<NativeLiveDevice>()
                    .expect("probe owns live device"),
                card_id: probe.id,
                position_mm: probe.position,
                rotation_millidegrees: probe.rotation,
            })
        };
        *probe.result.lock().expect("probe result lock") = Some(outcome);
        world.write_message(AppExit::Success);
    } else {
        world.insert_resource(probe);
    }
}

impl Probe {
    fn change(&mut self, stage: Stage) {
        self.stage = stage;
        self.entered = self.frame;
    }
    fn capture(&self, world: &mut World, name: &str) {
        world.resource_mut::<AcceptanceOptions>().screenshot = Some(self.directory.join(name));
        let mut clock = world.resource_mut::<LaunchClock>();
        clock.screenshot_requested = false;
        clock.screenshot_completed = false;
    }
    fn input(&mut self, world: &mut World, position: Vec2, action: PointerAction) {
        world.write_message(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: self.target.clone().expect("computed image target"),
                position,
            },
            action,
        ));
        self.pointer = position;
    }
    fn hovered(&self, world: &World, entity: Entity) -> bool {
        world
            .resource::<HoverMap>()
            .0
            .get(&PointerId::Mouse)
            .is_some_and(|hover| hover.contains_key(&entity))
    }
    fn advance(&mut self, world: &mut World) -> Result<(), String> {
        if matches!(self.stage, Stage::AfterCapture | Stage::FailureCapture) {
            return Ok(());
        }
        if self.frame - self.entered < 3 {
            return Ok(());
        }
        match self.stage {
            Stage::Warmup => {
                if self.frame < 60 {
                    return Ok(());
                }
                let controller = world.resource::<NativeController>();
                let id = controller
                    .scene
                    .cards
                    .iter()
                    .find(|card| card.face == Some(self.face))
                    .ok_or("owned render card missing")?
                    .id;
                let mut cards = world.query::<(Entity, &DraggableCard, &GlobalTransform)>();
                let (entity, _, transform) = cards
                    .iter(world)
                    .find(|(_, card, _)| card.0 == id)
                    .ok_or("owned card has no production drag target")?;
                self.card = Some(entity);
                let origin = transform.translation();
                let (yaw, pitch, roll) = transform.rotation().to_euler(EulerRot::YXZ);
                self.initial_rotation = [yaw, pitch, roll]
                    .map(|angle| (angle.to_degrees() * 1000.).round().rem_euclid(360000.) as i32);
                let mut zones = world.query::<(Entity, &CanonicalMirror, &GlobalTransform)>();
                let (play, _, transform) = zones
                    .iter(world)
                    .find(|(_, mirror, _)| mirror.id == ObjectId::Zone(ZoneId::Play))
                    .ok_or("production PLAY target is missing")?;
                self.play = Some(play);
                // The prior denied card deliberately stays at the centre.
                // Aim beside it for the legal drop, not through the same solid.
                let center = transform.translation()
                    + if self.expected == ExpectedPlay::Accepted {
                        Vec3::X * 0.08
                    } else {
                        Vec3::ZERO
                    };
                let mut cameras = world.query::<(
                    &Camera,
                    &GlobalTransform,
                    &RenderTarget,
                    Has<HandCamera>,
                    Has<TabletopCamera>,
                )>();
                for (camera, transform, target, hand, table) in cameras.iter(world) {
                    if hand && camera.is_active {
                        self.start = camera
                            .world_to_viewport(transform, origin)
                            .map_err(|_| "card is outside hand camera")?;
                        if !camera
                            .logical_viewport_rect()
                            .is_some_and(|rect| rect.contains(self.start))
                        {
                            return Err("owned card is not visible inside hand viewport".to_owned());
                        }
                        self.target = target.normalize(None);
                    }
                    if table {
                        self.destination = camera
                            .world_to_viewport(transform, center)
                            .map_err(|_| "PLAY is outside table camera")?;
                    }
                }
                if self.target.is_none() {
                    return Err("computed hand camera is missing".to_owned());
                }
                self.capture(world, "before.png");
                self.change(Stage::BeforeCapture);
            }
            Stage::BeforeCapture => {
                if !world.resource::<LaunchClock>().screenshot_completed {
                    return Ok(());
                }
                self.input(world, self.start, PointerAction::Move { delta: Vec2::ZERO });
                self.change(Stage::Hover);
            }
            Stage::Hover => {
                if !self.hovered(world, self.card.unwrap()) {
                    return Err("rendered hand card did not receive pointer hover".to_owned());
                }
                self.input(
                    world,
                    self.start,
                    PointerAction::Press(PointerButton::Primary),
                );
                self.change(Stage::Press);
            }
            Stage::Press => {
                let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
                for key in [KeyCode::ShiftLeft, KeyCode::ControlLeft, KeyCode::AltLeft] {
                    keys.press(key);
                }
                let delta = Vec2::new(12., -5.);
                self.input(world, self.start + delta, PointerAction::Move { delta });
                self.change(Stage::StartDrag);
            }
            Stage::StartDrag => {
                world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
                if world
                    .get::<DragPreview>(self.card.unwrap())
                    .is_none_or(|preview| preview.pointer != Some(PointerId::Mouse))
                {
                    return Err("rendered press/move did not capture the card".to_owned());
                }
                self.change(Stage::Move(1));
            }
            Stage::Move(step) => {
                let point =
                    (self.start + Vec2::new(12., -5.)).lerp(self.destination, f32::from(step) / 8.);
                self.input(
                    world,
                    point,
                    PointerAction::Move {
                        delta: point - self.pointer,
                    },
                );
                self.change(if step < 8 {
                    Stage::Move(step + 1)
                } else {
                    Stage::AwaitPose
                });
            }
            Stage::AwaitPose => {
                let preview = world
                    .get::<DragPreview>(self.card.unwrap())
                    .ok_or("dragged entity was replaced")?;
                self.position = preview
                    .physical_origin
                    .ok_or("drag never proposed a pose")?
                    .to_array()
                    .map(|v| (v * 1000.).round() as i32);
                self.rotation = preview
                    .physical_rotation
                    .ok_or("drag never proposed rotation")?;
                if self.rotation
                    != self
                        .initial_rotation
                        .map(|angle| (angle + 6000).rem_euclid(360000))
                {
                    return Err(
                        "rendered drag did not apply all three rotation modifiers".to_owned()
                    );
                }
                let observation = world.resource::<NativeLiveDevice>().observation();
                if observation.projection.current_revision != self.revision {
                    return Err("holding a card changed the logical revision".to_owned());
                }
                let accepted = observation
                    .physical_hands
                    .iter()
                    .find(|card| card.id == self.id)
                    .and_then(|card| card.pose.as_ref())
                    .is_some_and(|pose| {
                        pose.position_mm == self.position
                            && pose.rotation_millidegrees == self.rotation
                    });
                if !accepted {
                    return Ok(());
                }
                // A pointer over PLAY must put the card there too. Comparing
                // only accepted local/remote poses missed cross-camera drift.
                let mut cameras =
                    world.query_filtered::<(&Camera, &GlobalTransform), With<TabletopCamera>>();
                let (camera, transform) =
                    cameras.single(world).map_err(|_| "table camera missing")?;
                let rendered = camera
                    .world_to_viewport(
                        transform,
                        Vec3::from_array(self.position.map(|v| v as f32 / 1000.)),
                    )
                    .map_err(|_| "moved card is outside the table camera")?;
                if rendered.distance(self.destination) > 3. {
                    return Err("moved card drifted away from the pointer at PLAY".to_owned());
                }
                if !self.hovered(world, self.play.unwrap()) {
                    return Err("PLAY is occluded at the rendered drop location".to_owned());
                }
                self.capture(world, "held.png");
                self.change(Stage::HeldCapture);
            }
            Stage::HeldCapture => {
                if !world.resource::<LaunchClock>().screenshot_completed {
                    return Ok(());
                }
                // The final motion and release arrive together: do not wait
                // for a network receipt before trying to play the card.
                self.destination += Vec2::new(6., 0.);
                let mut cameras =
                    world.query_filtered::<(&Camera, &GlobalTransform), With<TabletopCamera>>();
                let (camera, transform) =
                    cameras.single(world).map_err(|_| "table camera missing")?;
                let ray = camera
                    .viewport_to_world(transform, self.destination)
                    .map_err(|_| "final pointer ray missing")?;
                let anchor = world
                    .get::<DragPreview>(self.card.unwrap())
                    .and_then(|preview| preview.physical_anchor)
                    .ok_or("held grab anchor missing")?;
                self.position = anchor
                    .project(ray)
                    .ok_or("final pointer missed drag plane")?
                    .to_array()
                    .map(|v| (v * 1000.).round() as i32);
                self.input(
                    world,
                    self.destination,
                    PointerAction::Move {
                        delta: self.destination - self.pointer,
                    },
                );
                self.input(
                    world,
                    self.destination,
                    PointerAction::Release(PointerButton::Primary),
                );
                self.change(Stage::Release);
            }
            Stage::Release => self.change(Stage::AwaitOutcome),
            Stage::AwaitOutcome => {
                let live = world.resource::<NativeLiveDevice>();
                let view = live.observation();
                let controller = world.resource::<NativeController>();
                match self.expected {
                    ExpectedPlay::Denied => {
                        if view.projection.current_revision != self.revision
                            || view.projection.payload.public_game_state != self.initial_game
                        {
                            return Err("out-of-turn drop mutated the rules state".to_owned());
                        }
                        if !controller.last_finding.starts_with("Not played:") {
                            return Err("out-of-turn drop did not retain visible denial feedback"
                                .to_owned());
                        }
                        if !view.physical_hands.iter().any(|card| {
                            card.id == self.id
                                && card.pose.as_ref().is_some_and(|pose| {
                                    pose.position_mm == self.position
                                        && pose.rotation_millidegrees == self.rotation
                                })
                        }) {
                            // Releasing does not wait for the final motion RPC.
                            // Observe its exact receipt within the probe's
                            // deadline, not a fixed number of rendered frames.
                            return Ok(());
                        }
                    }
                    ExpectedPlay::Accepted => {
                        if view.projection.current_revision == self.revision {
                            if controller
                                .last_finding
                                .starts_with("live play queue rejected:")
                            {
                                return Err(
                                    "immediate release lost the queued play intent".to_owned()
                                );
                            }
                            return Ok(());
                        }
                        if !view.physical_public.iter().any(|card| {
                            card.id == self.id
                                && card.face == self.face.code()
                                && card.pose.position_mm == self.position
                                && card.pose.rotation_millidegrees == self.rotation
                        }) || view.physical_hands.iter().any(|card| card.id == self.id)
                        {
                            return Err("legal drop did not publish the same moved card".to_owned());
                        }
                    }
                }
                self.capture(world, "after.png");
                self.change(Stage::AfterCapture);
            }
            Stage::AfterCapture | Stage::FailureCapture => unreachable!(),
        }
        Ok(())
    }
}

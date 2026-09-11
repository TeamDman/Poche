//! Native main-menu surface. Network owners consume requests asynchronously;
//! this plugin never loads credentials or starts network work on the UI thread.

use bevy::{
    clipboard::{Clipboard, ClipboardRead},
    prelude::*,
    text::{EditableText, TextCursorStyle, TextEdit},
};

mod clipboard;
use clipboard::InvitationClipboard;

#[cfg(feature = "input-probe")]
pub mod input_probe;

/// Both window and image-target UI use Bevy's picking pipeline. The legacy
/// `Interaction` focus system only handles Window render targets.
#[derive(Message)]
struct ButtonActivation(Entity);

fn activate_button(
    mut click: On<Pointer<Click>>,
    buttons: Query<(), Or<(With<Action>, With<LiveAction>, With<CopyInvitation>)>>,
    mut activations: MessageWriter<ButtonActivation>,
) {
    if click.button == bevy::picking::pointer::PointerButton::Primary
        && buttons.contains(click.entity)
    {
        click.propagate(false);
        activations.write(ButtonActivation(click.entity));
    }
}

#[derive(Message, Clone)]
pub enum DesktopMenuRequest {
    Create { name: String },
    Join { name: String, invitation: String },
}

/// Supplied by the transport's strict invitation parser. No clipboard bytes
/// are logged, and reading a valid invitation never automatically joins.
#[derive(Resource)]
pub struct InvitationValidator(pub fn(&str) -> bool);

#[derive(Resource, Default)]
pub struct DesktopMenuStatus {
    pub busy: bool,
    pub message: String,
}

/// Dedicated connection work, shared by Create and Join. No credential or
/// network operation runs in the Bevy frame schedule. Keep errors static and
/// redacted at this boundary.
#[derive(Resource)]
pub struct DesktopConnectionWorker {
    requests: std::sync::mpsc::SyncSender<DesktopMenuRequest>,
    results:
        std::sync::Mutex<std::sync::mpsc::Receiver<Result<crate::NativeLiveDevice, &'static str>>>,
}

impl DesktopConnectionWorker {
    pub fn start(
        mut connect: impl FnMut(DesktopMenuRequest) -> Result<crate::NativeLiveDevice, &'static str>
        + Send
        + 'static,
    ) -> Result<Self, &'static str> {
        let (request_tx, request_rx) = std::sync::mpsc::sync_channel(1);
        let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("poche-desktop-connect".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    if result_tx.send(connect(request)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| "connection worker could not start")?;
        Ok(Self {
            requests: request_tx,
            results: std::sync::Mutex::new(result_rx),
        })
    }
}

#[derive(Resource, Default)]
struct PendingClipboard(Option<ClipboardRead>);

#[derive(Component)]
pub struct DesktopMenuRoot;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Invitation,
}
#[derive(Component, Clone, Copy)]
enum Action {
    Create,
    Paste,
    Join,
}
#[derive(Component)]
struct StatusLabel;
#[derive(Component)]
struct LiveControls;
#[derive(Component)]
struct LiveAction(String);
#[derive(Component)]
struct CopyInvitation;

/// Installed by `run_menu` alongside the shared DesktopLiveControlsPlugin and
/// a transport-specific validator. The owning app removes DesktopMenuRoot
/// only when a room has actually joined.
pub struct DesktopMenuPlugin;
impl Plugin for DesktopMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DesktopMenuStatus>()
            .init_resource::<PendingClipboard>()
            .add_message::<DesktopMenuRequest>()
            .add_systems(Startup, setup.after(crate::setup_native_render_target))
            .add_systems(
                Update,
                (buttons, clipboard_result, connection, status_text).chain(),
            );
    }
}

/// The actual game controls are shared by menu-launched and directly attached
/// live devices, including windowless captures.
pub struct DesktopLiveControlsPlugin;
impl Plugin for DesktopLiveControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clipboard>()
            .init_resource::<InvitationClipboard>()
            .add_message::<ButtonActivation>()
            .add_observer(activate_button)
            .add_systems(
                Update,
                (
                    live_action_buttons,
                    refresh_live_controls,
                    live_finding_text,
                )
                    .chain()
                    .after(crate::poll_live_device),
            );
    }
}

fn refresh_live_controls(
    live: Option<Res<crate::NativeLiveDevice>>,
    mut fonts: ResMut<Assets<Font>>,
    mut card_font: Local<Option<Handle<Font>>>,
    mut revision: Local<Option<u64>>,
    old: Query<Entity, With<LiveControls>>,
    cameras: Query<Entity, With<crate::TabletopCamera>>,
    mut commands: Commands,
) {
    let Some(live) = live else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    let current = live.observation().projection.current_revision;
    if *revision == Some(current) {
        return;
    }
    *revision = Some(current);
    let card_font = card_font
        .get_or_insert_with(|| fonts.add(Font::from_bytes(crate::FONT_BYTES.to_vec())))
        .clone();
    for entity in &old {
        commands.entity(entity).despawn();
    }
    commands
        .spawn((
            LiveControls,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0.),
                width: percent(100.),
                padding: px(12.).all(),
                column_gap: px(8.),
                row_gap: px(8.),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            BackgroundColor(Color::srgb(0.04, 0.07, 0.08)),
        ))
        .with_children(|bar| {
            bar.spawn((
                LiveFinding,
                Text::new(""),
                TextFont::from_font_size(16.0).with_font(card_font.clone()),
                TextColor(Color::srgb(1.0, 0.9, 0.65)),
                Node { width: percent(100.), ..default() },
            ));
            bar.spawn((
                Text::new("Drag a hand card to move it. While dragging: Shift = yaw, Ctrl = pitch, Alt = roll."),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                Node { width: percent(100.), ..default() },
            ));
            if live.room_invitation().is_some() {
                bar.spawn((
                    Button,
                    CopyInvitation,
                    Node {
                        padding: px(10.).all(),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.14, 0.3, 0.32)),
                ))
                .with_child(Text::new("Copy lobby invitation"));
            }
            for action in &live.observation().actions {
                bar.spawn((
                    Button,
                    LiveAction(action.id.clone()),
                    Node {
                        padding: px(10.).all(),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.14, 0.3, 0.32)),
                ))
                .with_child((Text::new(&action.label), TextFont::from_font_size(20.0).with_font(card_font.clone())));
            }
        });
}

#[derive(Component)]
struct LiveFinding;

fn live_finding_text(
    controller: Option<Res<crate::NativeController>>,
    mut labels: Query<&mut Text, With<LiveFinding>>,
) {
    let Some(controller) = controller else {
        return;
    };
    for mut text in &mut labels {
        if text.0 != controller.last_finding {
            text.0.clone_from(&controller.last_finding);
        }
    }
}

#[test]
fn live_findings_are_visible_without_window_title_or_debug_overlay() {
    let mut app = App::new();
    let mut controller = crate::replay_fixture_controller().unwrap();
    controller.last_finding =
        "Not played: not your turn. Physical position is unchanged.".to_owned();
    app.insert_resource(controller);
    let label = app.world_mut().spawn((LiveFinding, Text::new(""))).id();
    app.add_systems(Update, live_finding_text);
    app.update();
    assert_eq!(
        app.world().get::<Text>(label).unwrap().0,
        "Not played: not your turn. Physical position is unchanged."
    );
}

fn live_action_buttons(
    mut activations: MessageReader<ButtonActivation>,
    buttons: Query<(Option<&LiveAction>, Has<CopyInvitation>)>,
    live: Option<ResMut<crate::NativeLiveDevice>>,
    controller: Option<ResMut<crate::NativeController>>,
    mut clipboard: ResMut<Clipboard>,
    mut invitation_clipboard: ResMut<InvitationClipboard>,
    mut confirmation: Local<Option<String>>,
) {
    let (Some(mut live), Some(mut controller)) = (live, controller) else {
        return;
    };
    for ButtonActivation(entity) in activations.read() {
        let Ok((action, copy)) = buttons.get(*entity) else {
            continue;
        };
        if copy {
            controller.last_finding = match live
                .room_invitation()
                .map(|code| invitation_clipboard.set_text(&mut clipboard, code.to_owned()))
            {
                Some(Ok(())) => "Lobby invitation copied.".to_owned(),
                _ => "Could not copy the lobby invitation.".to_owned(),
            };
        } else if let Some(action) = action {
            let destructive = live
                .observation()
                .actions
                .iter()
                .find(|candidate| candidate.id == action.0)
                .is_some_and(|candidate| {
                    matches!(
                        candidate.payload,
                        poche_protocol::CommandPayload::Leave
                            | poche_protocol::CommandPayload::CloseRoom
                    )
                });
            if destructive && confirmation.as_deref() != Some(&action.0) {
                *confirmation = Some(action.0.clone());
                controller.last_finding =
                    "Click the same action again to confirm leaving/closing the room.".to_owned();
                continue;
            }
            *confirmation = None;
            controller.last_finding = match live.submit_action(&action.0) {
                Ok(()) => "Action sent; awaiting the room.".to_owned(),
                Err(error) => error,
            };
        }
    }
}

fn connection(
    mut requests: MessageReader<DesktopMenuRequest>,
    worker: Option<Res<DesktopConnectionWorker>>,
    mut status: ResMut<DesktopMenuStatus>,
    roots: Query<Entity, With<DesktopMenuRoot>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let sent = worker
            .as_ref()
            .is_some_and(|worker| worker.requests.try_send(request.clone()).is_ok());
        if !sent {
            status.busy = false;
            status.message = "Connection service unavailable. Please try again.".to_owned();
        }
    }
    let Some(worker) = worker else {
        return;
    };
    let result = match worker.results.lock() {
        Ok(results) => match results.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) if status.busy => {
                Err("Connection worker stopped. Restart the client.")
            }
            Err(_) => return,
        },
        Err(_) => Err("Connection worker unavailable. Restart the client."),
    };
    match result {
        Ok(live) => match crate::native_controller_from_observation(live.observation()) {
            Ok(controller) => {
                for entity in &roots {
                    commands.entity(entity).despawn();
                }
                commands.insert_resource(controller);
                commands.insert_resource(live);
                commands.run_system_cached(crate::setup_native_scene);
                status.busy = false;
            }
            Err(_) => {
                status.busy = false;
                status.message = "The room returned an invalid table view.".to_owned();
            }
        },
        Err(message) => {
            status.busy = false;
            status.message = message.to_owned();
        }
    }
}

fn setup(mut commands: Commands, surface: Res<crate::NativeRenderSurface>) {
    let mut camera = commands.spawn((Camera2d, DesktopMenuRoot));
    if let Some(target) = surface.render_target() {
        camera.insert(target);
    }
    let camera = camera.id();
    commands
        .spawn((
            DesktopMenuRoot,
            UiTargetCamera(camera),
            Node {
                width: percent(100.),
                height: percent(100.),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14.),
                ..default()
            },
            BackgroundColor(Color::srgb(0.035, 0.055, 0.07)),
        ))
        .with_children(|parent| {
            parent.spawn((Text::new("POCHE"), TextFont::from_font_size(48.)));
            parent.spawn(Text::new("Your name"));
            parent.spawn((
                Field::Name,
                Node {
                    width: px(360.),
                    padding: px(10.).all(),
                    ..default()
                },
                EditableText {
                    max_characters: Some(48),
                    ..default()
                },
                TextCursorStyle::default(),
                TextFont::from_font_size(24.),
                BackgroundColor(Color::srgb(0.13, 0.17, 0.2)),
            ));
            for (label, action) in [
                ("Create lobby", Action::Create),
                ("Paste invitation from clipboard", Action::Paste),
            ] {
                parent
                    .spawn((
                        Button,
                        action,
                        Node {
                            padding: px(14.).all(),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.32, 0.3)),
                    ))
                    .with_child((Text::new(label), TextFont::from_font_size(24.)));
            }
            parent.spawn(Text::new("Invitation (paste or edit, then choose Join)"));
            parent.spawn((
                Field::Invitation,
                Node {
                    width: px(500.),
                    max_width: percent(90.),
                    padding: px(10.).all(),
                    ..default()
                },
                EditableText {
                    max_characters: Some(4096),
                    visible_lines: Some(3.),
                    allow_newlines: true,
                    ..default()
                },
                TextCursorStyle::default(),
                TextFont::from_font_size(18.),
                BackgroundColor(Color::srgb(0.13, 0.17, 0.2)),
            ));
            parent
                .spawn((
                    Button,
                    Action::Join,
                    Node {
                        padding: px(14.).all(),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.32, 0.3)),
                ))
                .with_child((Text::new("Join"), TextFont::from_font_size(24.)));
            parent.spawn((StatusLabel, Text::new(""), TextFont::from_font_size(18.)));
        });
}

fn buttons(
    mut activations: MessageReader<ButtonActivation>,
    buttons: Query<&Action>,
    fields: Query<(&Field, &EditableText)>,
    mut clipboard: ResMut<Clipboard>,
    mut invitation_clipboard: ResMut<InvitationClipboard>,
    mut pending: ResMut<PendingClipboard>,
    validator: Res<InvitationValidator>,
    mut status: ResMut<DesktopMenuStatus>,
    mut requests: MessageWriter<DesktopMenuRequest>,
) {
    for ButtonActivation(entity) in activations.read() {
        let Ok(action) = buttons.get(*entity) else {
            continue;
        };
        if status.busy {
            continue;
        }
        if matches!(action, Action::Paste) {
            pending.0 = Some(invitation_clipboard.fetch_text(&mut clipboard));
            continue;
        }
        let value = |which| {
            fields
                .iter()
                .find(|(field, _)| **field == which)
                .map(|(_, text)| text.value().into_iter().collect::<String>())
                .unwrap_or_default()
        };
        let name = value(Field::Name).trim().to_owned();
        if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
            status.message = "Enter a name first.".to_owned();
            continue;
        }
        match action {
            Action::Create => {
                requests.write(DesktopMenuRequest::Create { name });
            }
            Action::Join => {
                let invitation = value(Field::Invitation).trim().to_owned();
                if invitation.len() > 4096 || !(validator.0)(&invitation) {
                    status.message = "That invitation is not valid.".to_owned();
                    continue;
                }
                requests.write(DesktopMenuRequest::Join { name, invitation });
            }
            Action::Paste => unreachable!(),
        }
        status.busy = true;
        status.message = "Connecting...".to_owned();
    }
}

fn clipboard_result(
    mut pending: ResMut<PendingClipboard>,
    validator: Res<InvitationValidator>,
    mut fields: Query<(&Field, &mut EditableText)>,
    mut status: ResMut<DesktopMenuStatus>,
) {
    let Some(result) = pending.0.as_mut().and_then(ClipboardRead::poll_result) else {
        return;
    };
    pending.0 = None;
    if status.busy {
        return;
    }
    match result {
        Ok(text) if text.len() <= 4096 && (validator.0)(text.trim()) => {
            for (field, mut input) in &mut fields {
                if *field == Field::Invitation {
                    // Replacing EditableText's backing buffer can leave its
                    // cached rendered layout untouched. Use the same editing
                    // pipeline as keyboard input and preserve field settings.
                    input.queue_edit(TextEdit::SelectAll);
                    input.queue_edit(TextEdit::Insert(text.trim().into()));
                }
            }
            status.message = "Invitation pasted. Choose Join when ready.".to_owned();
        }
        _ => status.message = "No valid lobby invitation found in the clipboard.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_failure_is_reported_without_blocking_the_frame() {
        let frame_thread = std::thread::current().id();
        let worker = DesktopConnectionWorker::start(move |_| {
            assert_ne!(std::thread::current().id(), frame_thread);
            Err("Network unavailable. Try again.")
        })
        .unwrap();
        let mut app = App::new();
        app.insert_resource(worker)
            .insert_resource(DesktopMenuStatus {
                busy: true,
                message: "Connecting…".to_owned(),
            })
            .add_message::<DesktopMenuRequest>()
            .add_systems(Update, connection);
        app.world_mut()
            .resource_mut::<Messages<DesktopMenuRequest>>()
            .write(DesktopMenuRequest::Create {
                name: "Alice".to_owned(),
            });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.update();
            if !app.world().resource::<DesktopMenuStatus>().busy {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not report its result"
            );
            std::thread::yield_now();
        }
        assert_eq!(
            app.world().resource::<DesktopMenuStatus>().message,
            "Network unavailable. Try again."
        );
        assert!(!app.world().contains_resource::<crate::NativeController>());
    }

    #[test]
    fn join_requires_explicit_press_and_busy_state_blocks_duplicate_requests() {
        let mut app = App::new();
        app.insert_resource(InvitationValidator(|text| text == "valid-test-invitation"))
            .init_resource::<DesktopMenuStatus>()
            .init_resource::<PendingClipboard>()
            .init_resource::<Clipboard>()
            .init_resource::<InvitationClipboard>()
            .add_message::<ButtonActivation>()
            .add_message::<DesktopMenuRequest>()
            .add_systems(Update, buttons);
        app.world_mut()
            .spawn((Field::Name, EditableText::new(" Alice ")));
        app.world_mut().spawn((
            Field::Invitation,
            EditableText::new("valid-test-invitation"),
        ));
        let button = app.world_mut().spawn(Action::Join).id();
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<DesktopMenuRequest>>()
                .drain()
                .next()
                .is_none()
        );
        app.world_mut().write_message(ButtonActivation(button));
        app.update();
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<DesktopMenuRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert!(
            matches!(&requests[0], DesktopMenuRequest::Join { name, invitation } if name == "Alice" && invitation == "valid-test-invitation")
        );
        assert!(app.world().resource::<DesktopMenuStatus>().busy);
        app.update();
        app.world_mut().write_message(ButtonActivation(button));
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<DesktopMenuRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }

    #[test]
    fn clipboard_prefill_requires_valid_invitation_and_does_not_connect() {
        let mut app = App::new();
        app.insert_resource(InvitationValidator(|text| text == "valid-test-invitation"))
            .init_resource::<DesktopMenuStatus>()
            .insert_resource(PendingClipboard(Some(ClipboardRead::Ready(Ok(
                "unrelated clipboard".to_owned(),
            )))))
            .add_systems(Update, clipboard_result);
        let field = app
            .world_mut()
            .spawn((Field::Invitation, EditableText::new("existing")))
            .id();
        app.update();
        let value = |app: &App| {
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .into_iter()
                .collect::<String>()
        };
        assert_eq!(value(&app), "existing");
        app.world_mut().resource_mut::<PendingClipboard>().0 =
            Some(ClipboardRead::Ready(Ok("valid-test-invitation".to_owned())));
        app.update();
        // This narrow unit test does not install Bevy's text-edit/layout
        // pipeline. The GPU input harness proves these queued edits become
        // both the submitted field value and visible glyphs.
        let input = app.world().get::<EditableText>(field).unwrap();
        assert!(
            matches!(input.pending_edits.as_slice(), [TextEdit::TextEnd(false), TextEdit::SelectAll, TextEdit::Insert(text)] if text == "valid-test-invitation")
        );
        assert!(!app.world().resource::<DesktopMenuStatus>().busy);
    }
}

fn status_text(status: Res<DesktopMenuStatus>, mut labels: Query<&mut Text, With<StatusLabel>>) {
    if status.is_changed() {
        for mut label in &mut labels {
            label.0.clone_from(&status.message);
        }
    }
}

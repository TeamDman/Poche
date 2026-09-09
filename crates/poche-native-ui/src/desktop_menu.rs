//! Native main-menu surface. Network owners consume requests asynchronously;
//! this plugin never loads credentials or starts network work on the UI thread.

use bevy::{
    clipboard::{Clipboard, ClipboardRead},
    prelude::*,
    text::{EditableText, TextCursorStyle},
};

#[derive(Message)]
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

/// Install alongside DefaultPlugins and a transport-specific validator.
/// The owning app removes DesktopMenuRoot when a room has actually joined.
pub struct DesktopMenuPlugin;
impl Plugin for DesktopMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DesktopMenuStatus>()
            .init_resource::<PendingClipboard>()
            .add_message::<DesktopMenuRequest>()
            .add_systems(Startup, setup)
            .add_systems(Update, (buttons, clipboard_result, status_text).chain());
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, DesktopMenuRoot));
    commands
        .spawn((
            DesktopMenuRoot,
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
                ("Join lobby — read invitation from clipboard", Action::Paste),
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
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    fields: Query<(&Field, &EditableText)>,
    mut clipboard: ResMut<Clipboard>,
    mut pending: ResMut<PendingClipboard>,
    validator: Res<InvitationValidator>,
    mut status: ResMut<DesktopMenuStatus>,
    mut requests: MessageWriter<DesktopMenuRequest>,
) {
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed || status.busy {
            continue;
        }
        if matches!(action, Action::Paste) {
            pending.0 = Some(clipboard.fetch_text());
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
        status.message = "Connecting…".to_owned();
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
                    *input = EditableText {
                        max_characters: Some(4096),
                        visible_lines: Some(3.),
                        allow_newlines: true,
                        ..EditableText::new(text.trim())
                    };
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
    fn join_requires_explicit_press_and_busy_state_blocks_duplicate_requests() {
        let mut app = App::new();
        app.insert_resource(InvitationValidator(|text| text == "valid-test-invitation"))
            .init_resource::<DesktopMenuStatus>()
            .init_resource::<PendingClipboard>()
            .init_resource::<Clipboard>()
            .add_message::<DesktopMenuRequest>()
            .add_systems(Update, buttons);
        app.world_mut()
            .spawn((Field::Name, EditableText::new(" Alice ")));
        app.world_mut().spawn((
            Field::Invitation,
            EditableText::new("valid-test-invitation"),
        ));
        let button = app
            .world_mut()
            .spawn((Action::Join, Interaction::None))
            .id();
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<DesktopMenuRequest>>()
                .drain()
                .next()
                .is_none()
        );
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
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
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::None;
        app.update();
        *app.world_mut().get_mut::<Interaction>(button).unwrap() = Interaction::Pressed;
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
        assert_eq!(value(&app), "valid-test-invitation");
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

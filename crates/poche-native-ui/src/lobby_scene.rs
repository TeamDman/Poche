// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/

//! Public membership mirror and seat affordances. No seating authority lives here.
use super::*;

#[derive(Component)]
pub(crate) struct SeatControl(pub SeatId);
#[derive(Component)]
struct LobbyRoot;
#[derive(Component)]
struct LobbyLabel(Vec3);
#[derive(Component)]
struct ParticipantMirror;

pub(crate) struct LobbyScenePlugin;
impl Plugin for LobbyScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (refresh, position_labels)
                .chain()
                .after(poll_live_device)
                .run_if(resource_exists::<NativeController>),
        );
    }
}

/// Resolve either the chair mesh or its label through the exact action palette.
pub(crate) fn click_seat(
    mut click: On<Pointer<Click>>,
    seats: Query<&SeatControl>,
    live: Option<ResMut<NativeLiveDevice>>,
    mut controller: ResMut<NativeController>,
) {
    if click.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok(seat) = seats.get(click.entity) else {
        return;
    };
    click.propagate(false);
    let Some(mut live) = live else {
        return;
    };
    let action = live.observation().actions.iter().find(|action|
        matches!(action.payload, CommandPayload::TakeSeat { seat: ordinal } if ordinal == seat.0.get()))
        .map(|action| action.id.clone());
    controller.last_finding = match action {
        Some(action) => match live.submit_action(&action) {
            Ok(()) => "Seat request sent; waiting for the room.".to_owned(),
            Err(error) => error,
        },
        None => "That seat is not currently available to you.".to_owned(),
    };
}

fn short_name(name: &str) -> String {
    if name.chars().count() <= 18 {
        return name.to_owned();
    }
    format!("{}…", name.chars().take(17).collect::<String>())
}

fn refresh(
    mut commands: Commands,
    controller: Res<NativeController>,
    live: Option<Res<NativeLiveDevice>>,
    mut previous: Local<Option<(LayoutId, Vec<poche_ui::ParticipantAnchor>, Vec<u8>)>>,
    roots: Query<Entity, With<LobbyRoot>>,
    cameras: Query<Entity, With<DesktopControlsCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fonts: ResMut<Assets<Font>>,
    mut font: Local<Option<Handle<Font>>>,
) {
    let Some(people) = &controller.participants else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    let available: Vec<_> = live
        .iter()
        .flat_map(|live| live.observation().actions.iter())
        .filter_map(|action| {
            if let CommandPayload::TakeSeat { seat } = action.payload {
                Some(seat)
            } else {
                None
            }
        })
        .collect();
    if previous
        .as_ref()
        .is_some_and(|(layout, prior, prior_available)| {
            *layout == controller.layout.id() && prior == people && prior_available == &available
        })
    {
        return;
    }
    *previous = Some((controller.layout.id(), people.clone(), available.clone()));
    let font = font
        .get_or_insert_with(|| fonts.add(Font::from_bytes(FONT_BYTES.to_vec())))
        .clone();
    for root in &roots {
        commands.entity(root).despawn();
    }
    for person in people {
        let color = if !person.connected {
            Color::srgb(0.4, 0.42, 0.44)
        } else if person.ready {
            Color::srgb(0.18, 0.68, 0.38)
        } else {
            Color::srgb(0.18, 0.43, 0.75)
        };
        commands.spawn((
            LobbyRoot,
            ParticipantMirror,
            Name::new(person.principal.clone()),
            Mesh3d(meshes.add(Capsule3d::new(0.06, 0.24))),
            MeshMaterial3d(materials.add(color)),
            pose_transform(person.pose),
            Pickable::IGNORE,
        ));
        let state = if !person.connected {
            "disconnected"
        } else if person.ready {
            "ready"
        } else {
            "not ready"
        };
        let place = person.seat.map_or_else(
            || "watching".to_owned(),
            |seat| format!("seat {}", seat.get()),
        );
        label(
            &mut commands,
            camera,
            point_to_vec3(person.pose.translation) + Vec3::Y * 0.25,
            format!("{}\n{place} · {state}", short_name(&person.display_name)),
            None,
            &font,
        );
    }
    for placement in controller.layout.seats() {
        if people
            .iter()
            .any(|person| person.seat == Some(placement.seat))
        {
            continue;
        }
        label(
            &mut commands,
            camera,
            point_to_vec3(placement.seat_pose.translation) + Vec3::Y * 0.1,
            format!(
                "Seat {} · empty\n{}",
                placement.seat.get(),
                if available.contains(&placement.seat.get()) {
                    "Click to sit"
                } else {
                    "Unavailable now"
                }
            ),
            available
                .contains(&placement.seat.get())
                .then_some(placement.seat),
            &font,
        );
    }
}

fn label(
    commands: &mut Commands,
    camera: Entity,
    anchor: Vec3,
    text: String,
    seat: Option<SeatId>,
    font: &Handle<Font>,
) {
    let mut entity = commands.spawn((
        LobbyRoot,
        LobbyLabel(anchor),
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: px(180.),
            padding: px(4.).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.06, 0.075, 0.92)),
    ));
    if let Some(seat) = seat {
        entity
            .insert((Button, SeatControl(seat)))
            .observe(click_seat);
    } else {
        entity.insert(Pickable::IGNORE);
    }
    entity.with_child((
        Text::new(text),
        TextFont::from_font_size(15.).with_font(font.clone()),
        TextColor(Color::WHITE),
        Pickable::IGNORE,
    ));
}

fn position_labels(
    camera: Single<(&Camera, &GlobalTransform), With<TabletopCamera>>,
    mut labels: Query<(&LobbyLabel, &mut Node)>,
) {
    let (camera, transform) = *camera;
    for (label, mut node) in &mut labels {
        let point = camera.world_to_viewport(transform, label.0).ok();
        let viewport = camera.logical_viewport_rect();
        if let (Some(point), Some(viewport)) = (point, viewport) {
            if viewport.contains(point) && viewport.width() >= 180. && viewport.height() >= 56. {
                node.display = Display::Flex;
                node.left = px((point.x - 90.).clamp(viewport.min.x, viewport.max.x - 180.));
                node.top = px((point.y - 56.).clamp(viewport.min.y, viewport.max.y - 56.));
                continue;
            }
        }
        node.display = Display::None;
    }
}

/// Inspect the actual mirror entities after Bevy propagation, not just source data.
#[cfg(feature = "input-probe")]
pub(crate) fn validate_members(world: &mut World) -> Result<(), String> {
    let expected = world
        .resource::<NativeController>()
        .participants
        .clone()
        .ok_or("live membership missing")?;
    let mut people = world.query_filtered::<(&Name, &GlobalTransform), With<ParticipantMirror>>();
    if people.iter(world).count() != expected.len() {
        return Err("rendered participant count disagrees with membership".to_owned());
    }
    for person in &expected {
        let (_, transform) = people
            .iter(world)
            .find(|(name, _)| name.as_str() == person.principal)
            .ok_or("member's rendered capsule missing")?;
        if transform
            .translation()
            .distance(point_to_vec3(person.pose.translation))
            > 0.0001
        {
            return Err("rendered capsule disagrees with public anchor".to_owned());
        }
    }
    if world
        .query::<&CanonicalMirror>()
        .iter(world)
        .any(|mirror| matches!(mirror.id, ObjectId::Player(_)))
    {
        return Err("live room rendered a placeholder player slot".to_owned());
    }
    Ok(())
}

#[cfg(all(test, feature = "input-probe"))]
mod tests {
    use super::*;
    use poche_player_client::{LoopbackDeviceTransport, PlayerDeviceClient};
    use poche_protocol::{CommandId, PrincipalId, RoomId};
    use poche_runtime::{
        LoopbackCodec, OracleRoomActionSource, OracleSessionGame, RuntimeLoopbackDeviceAdapter,
    };
    use poche_session::SessionState;

    #[test]
    #[ignore = "windowless GPU lobby input; set fresh POCHE_LOBBY_EVIDENCE_ROOT"]
    fn rendered_lobby_seat_ready_and_release() {
        let root = std::env::var_os("POCHE_LOBBY_EVIDENCE_ROOT").expect("fresh evidence root");
        let room = RoomId::new("rendered-lobby-membership").unwrap();
        let player = PrincipalId::new("lobby-pilot").unwrap();
        let profile = crate::tests::certified_profile(&player, "lobby-pilot", "77");
        let state = SessionState::<OracleSessionGame<2>>::pending(
            room.clone(),
            PrincipalId::new("clock").unwrap(),
            PrincipalId::new("game").unwrap(),
        );
        let adapter = RuntimeLoopbackDeviceAdapter::new(
            state,
            OracleRoomActionSource::new(29, 2, "invite", 30, "countdown").unwrap(),
            LoopbackCodec::Typed,
        );
        adapter.enroll(&profile).unwrap();
        let mut client =
            Some(PlayerDeviceClient::new(profile, LoopbackDeviceTransport::new(adapter)).unwrap());
        let room_for_worker = room.clone();
        let worker = crate::desktop_menu::DesktopConnectionWorker::start(move |request| {
            if !matches!(
                request,
                crate::desktop_menu::DesktopMenuRequest::Create { .. }
            ) {
                return Err("expected Create");
            }
            let mut client = client.take().ok_or("already created")?;
            let before = client.observe(&room_for_worker).map_err(|_| "observe")?;
            client
                .invoke_payload(
                    &before,
                    &CommandPayload::CreateRoom,
                    CommandId::new("create-lobby").unwrap(),
                )
                .map_err(|_| "create")?;
            let mut live = NativeLiveDevice::connect(client, room_for_worker.clone())
                .map_err(|_| "connect")?;
            live.set_room_invitation("isolated-test-invitation".to_owned());
            Ok(live)
        })
        .unwrap();
        let outcome = crate::desktop_menu::input_probe::run(
            worker,
            crate::desktop_menu::InvitationValidator(|text| text == "isolated-test-invitation"),
            "Lobby Pilot",
            crate::desktop_menu::input_probe::MenuScenario::CreateAndSeat,
            crate::desktop_menu::input_probe::IsolatedClipboard::default(),
            std::path::Path::new(&root),
        )
        .unwrap()
        .unwrap();
        let view = outcome.live.observation();
        assert_eq!(view.projection.payload.members.len(), 1);
        assert!(view.projection.payload.members[0].seat.is_none());
        assert!(view.projection.payload.own_hand.is_none());
        assert!(view.projection.payload.granted_hands.is_empty());
        let controller = native_controller_from_observation(view).unwrap();
        let people = controller.participants.unwrap();
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].principal, player.as_str());
        assert!(people[0].seat.is_none());
    }
}

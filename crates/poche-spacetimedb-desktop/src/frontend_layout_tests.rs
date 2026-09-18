// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exercise the production menu with Bevy's real font measurement and UI layout.
//! Ordinary regressions do not use a GPU. An ignored visual check renders into
//! images; neither path opens native windows, connects to an authority, or reads
//! user identity files.

use super::*;
use bevy::ecs::system::RunSystemOnce;

struct LayoutShaderAssets;

impl Plugin for LayoutShaderAssets {
    fn build(&self, app: &mut App) {
        // Renderer-free layout still registers shader assets requested by the
        // ordinary presentation plugins, but never compiles or executes them.
        app.init_asset::<bevy::shader::Shader>()
            .init_asset_loader::<bevy::shader::ShaderLoader>();
    }
}

struct MenuLayout {
    title: Rect,
    identity: Rect,
    previous: Rect,
    next: Rect,
    viewport: Vec2,
    identity_labels: Vec<ButtonLabelLayout>,
}

struct ButtonLabelLayout {
    text: String,
    button: Rect,
    bounds: Rect,
    glyph_count: usize,
}

fn menu_app(
    size: UVec2,
    scale: f32,
    history_count: usize,
    label: &str,
    render: bool,
) -> (App, Option<Handle<Image>>) {
    let directory = tempfile::tempdir().expect("disposable menu fixture");
    let authority = AuthorityEndpoint::select(Some("maincloud"), None).unwrap();
    let mut vault = IdentityVault::load(directory.path().join("identities.json")).unwrap();
    let account = vault
        .create(label, &authority.uri, &authority.database)
        .unwrap();
    for index in 0..history_count {
        vault
            .remember_lobby(
                &account.account_id,
                &format!("layout-room-{index}"),
                &format!("PCH-0000-0000-0000-{index:04X}"),
            )
            .unwrap();
    }
    let state = UiState {
        screen: UiScreen::MainMenu,
        active_account_id: Some(account.account_id),
        account_label: label.into(),
        display_name: label.into(),
        status: "Ready to create or join a lobby.".into(),
        ..UiState::for_authority(&authority)
    };
    let mut app = App::new();
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        .disable::<WinitPlugin>()
        .disable::<LogPlugin>()
        .disable::<bevy::audio::AudioPlugin>();
    if render {
        plugins = plugins.set(RenderPlugin {
            render_creation: WgpuSettings {
                backends: GraphicsBackend::default().backends(),
                ..default()
            }
            .into(),
            synchronous_pipeline_compilation: true,
            ..default()
        });
    } else {
        plugins = plugins
            .disable::<RenderPlugin>()
            .add_before::<bevy::core_pipeline::CorePipelinePlugin>(LayoutShaderAssets);
    }
    app.add_plugins(plugins);
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(size.x, size.y).with_scale_factor_override(scale),
            ..default()
        },
        PrimaryWindow,
    ));
    let camera = app
        .world_mut()
        .spawn((
            Camera2d,
            IsDefaultUiCamera,
            Camera {
                computed: bevy::camera::ComputedCameraValues {
                    target_info: Some(bevy::camera::RenderTargetInfo {
                        physical_size: size,
                        scale_factor: scale,
                    }),
                    ..default()
                },
                viewport: Some(Viewport {
                    physical_size: size,
                    ..default()
                }),
                ..default()
            },
        ))
        .id();
    let target = render.then(|| {
        let mut image = Image::new_uninit(
            Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage |=
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.world_mut()
            .entity_mut(camera)
            .insert(RenderTarget::Image(target.clone().into()));
        // Image targets have scale factor 1. UiScale reproduces the same physical
        // font and control sizes as a window with the requested display scaling.
        app.insert_resource(UiScale(scale));
        target
    });
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(FONT_BYTES.to_vec()));
    app.insert_resource(PocheFont(font));
    app.add_systems(Update, apply_poche_font);
    app.world_mut()
        .run_system_once(move |mut commands: Commands| {
            spawn_frontend(&mut commands, camera, &authority, &state, &vault, false);
        })
        .unwrap();
    while app.plugins_state() != bevy::app::PluginsState::Ready {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    // Font loading, intrinsic text measurements, and the resulting layout settle
    // over successive schedules, just as in a rendered app.
    for _ in 0..8 {
        app.update();
    }
    (app, target)
}

fn menu_layout(size: UVec2, scale: f32, history_count: usize, label: &str) -> MenuLayout {
    let (mut app, _) = menu_app(size, scale, history_count, label, false);
    measure_menu(&mut app, size)
}

fn measure_menu(app: &mut App, size: UVec2) -> MenuLayout {
    let world = app.world_mut();
    let title = world
        .query::<(&Text, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
        .find(|(text, _, _)| text.0 == "POCHE")
        .map(|(_, node, transform)| rect(node, transform))
        .expect("production title");
    let mut identity = None;
    let mut previous = None;
    let mut next = None;
    let mut identity_labels = Vec::new();
    for (action, node, transform, children) in world
        .query::<(&UiAction, &ComputedNode, &UiGlobalTransform, &Children)>()
        .iter(world)
    {
        match action {
            UiAction::OpenIdentities => identity = Some(rect(node, transform)),
            UiAction::PreviousIdentity => previous = Some(rect(node, transform)),
            UiAction::NextIdentity => next = Some(rect(node, transform)),
            _ => {}
        }
        if matches!(
            action,
            UiAction::OpenIdentities | UiAction::PreviousIdentity | UiAction::NextIdentity
        ) {
            let label = children
                .iter()
                .find_map(|child| world.get::<Text>(child).map(|text| (child, text)))
                .expect("identity button text child");
            let text_node = world
                .get::<ComputedNode>(label.0)
                .expect("identity text node");
            let text_transform = world
                .get::<UiGlobalTransform>(label.0)
                .expect("identity text transform");
            let glyph_count = world
                .get::<bevy::text::TextLayoutInfo>(label.0)
                .expect("identity shaped text")
                .glyphs
                .len();
            identity_labels.push(ButtonLabelLayout {
                text: label.1.0.clone(),
                button: rect(node, transform),
                bounds: rect(text_node, text_transform),
                glyph_count,
            });
        }
    }
    MenuLayout {
        title,
        identity: identity.expect("identity selector"),
        previous: previous.expect("previous identity"),
        next: next.expect("next identity"),
        viewport: size.as_vec2(),
        identity_labels,
    }
}

#[derive(Resource, Default)]
struct CaptureFinished(bool);

fn capture_menu(app: &mut App, target: &Handle<Image>, name: &str) {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/poche-puppet/title-layout")
        .join(name);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    app.insert_resource(CaptureFinished::default());
    let mut save = bevy::render::view::screenshot::save_to_disk(output.clone());
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(
            move |capture: On<bevy::render::view::screenshot::ScreenshotCaptured>,
                  mut finished: ResMut<CaptureFinished>| {
                // Use Bevy's same GPU screenshot path as the live file-control app.
                save(capture);
                finished.0 = true;
            },
        );
    let deadline = Instant::now() + Duration::from_secs(20);
    while !app.world().resource::<CaptureFinished>().0 {
        assert!(Instant::now() < deadline, "GPU capture timed out");
        app.update();
        std::thread::sleep(Duration::from_millis(8));
    }
    assert!(output.is_file(), "GPU capture did not write {output:?}");
    println!("Title layout capture: {}", output.display());
}

#[test]
#[ignore = "windowless GPU screenshots; run explicitly for visual verification"]
fn capture_title_identity_layout() {
    for (size, scale, label, name) in [
        (UVec2::new(1770, 1140), 1.5, "Alice", "title-150dpi.png"),
        (UVec2::new(1180, 760), 1., "Alice", "title-100dpi.png"),
        (
            UVec2::new(640, 600),
            1.,
            "An identity with a long name",
            "title-narrow.png",
        ),
    ] {
        let (mut app, target) = menu_app(size, scale, 4, label, true);
        for _ in 0..20 {
            app.update();
            std::thread::sleep(Duration::from_millis(8));
        }
        assert_identity_and_title_are_separate(&measure_menu(&mut app, size));
        capture_menu(&mut app, target.as_ref().unwrap(), name);
        if size.x == 640 {
            // Scrolling is exercised against the production viewport, not a
            // second synthetic layout assembled only for the capture.
            for _ in 0..3 {
                page(&mut app, KeyCode::PageDown);
            }
            capture_menu(
                &mut app,
                target.as_ref().unwrap(),
                "title-narrow-scrolled.png",
            );
        }
    }
}

fn rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_center_size(transform.translation, node.size())
}

fn assert_identity_and_title_are_separate(layout: &MenuLayout) {
    assert!(layout.title.width() > 10. && layout.title.height() > 10.);
    assert!(layout.identity.width() > 10. && layout.identity.height() > 10.);
    assert_eq!(layout.identity_labels.len(), 3);
    for label in &layout.identity_labels {
        assert!(
            label.bounds.width() > 1. && label.bounds.height() > 1. && label.glyph_count > 0,
            "identity label {:?} is not rendered: bounds {:?}, {} glyphs",
            label.text,
            label.bounds,
            label.glyph_count
        );
        assert!(
            label.bounds.min.x >= label.button.min.x - 1.
                && label.bounds.max.x <= label.button.max.x + 1.
                && label.bounds.min.y >= label.button.min.y - 1.
                && label.bounds.max.y <= label.button.max.y + 1.,
            "identity label {:?} {:?} leaves button {:?}",
            label.text,
            label.bounds,
            label.button,
        );
    }
    assert!(
        layout.title.min.y >= layout.identity.max.y + 1.,
        "title {:?} overlaps identity {:?}",
        layout.title,
        layout.identity
    );
    for (name, bounds) in [
        ("title", layout.title),
        ("identity", layout.identity),
        ("previous", layout.previous),
        ("next", layout.next),
    ] {
        assert!(
            bounds.min.x >= 0. && bounds.max.x <= layout.viewport.x,
            "{name} leaves viewport: {bounds:?}, {:?}",
            layout.viewport
        );
        assert!(bounds.min.y >= 0., "{name} is clipped above the viewport");
    }
}

#[test]
fn title_identity_overlap_user_screenshot_four_recent_lobbies_at_150_percent_dpi() {
    assert_identity_and_title_are_separate(&menu_layout(UVec2::new(1770, 1140), 1.5, 4, "Alice"));
}

#[test]
fn title_identity_overlap_neighbour_without_recent_lobbies() {
    assert_identity_and_title_are_separate(&menu_layout(UVec2::new(1770, 1140), 1.5, 0, "Alice"));
}

#[test]
fn title_identity_overlap_four_recent_lobbies_at_100_percent_dpi() {
    assert_identity_and_title_are_separate(&menu_layout(UVec2::new(1180, 760), 1., 4, "Alice"));
}

#[test]
fn title_identity_overlap_narrow_window_long_identity() {
    assert_identity_and_title_are_separate(&menu_layout(
        UVec2::new(640, 600),
        1.,
        4,
        "An identity with a long name",
    ));
}

#[test]
fn title_identity_overlap_480px_window_wraps_maximum_length_unbroken_identity() {
    let layout = menu_layout(
        UVec2::new(480, 600),
        1.,
        4,
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef",
    );
    assert_identity_and_title_are_separate(&layout);
    let label = layout
        .identity_labels
        .iter()
        .find(|label| label.text == "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef")
        .unwrap();
    assert!(
        label.bounds.height() > 25.,
        "maximum-length identity should wrap inside the narrow button"
    );
}

fn page(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.world_mut().run_system_once(scroll_frontend).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(key);
    app.update();
}

fn wheel(app: &mut App, unit: MouseScrollUnit, vertical: f32) {
    {
        let mut scroll = app.world_mut().resource_mut::<AccumulatedMouseScroll>();
        scroll.unit = unit;
        scroll.delta = Vec2::new(0., vertical);
    }
    app.world_mut().run_system_once(scroll_frontend).unwrap();
    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::ZERO;
    app.update();
}

#[test]
fn title_identity_overlap_wheel_lines_and_pixels_agree_at_high_dpi_and_clamp() {
    let size = UVec2::new(960, 900);
    let (mut app, _) = menu_app(size, 1.5, 4, "Alice", false);
    let header = measure_menu(&mut app, size).identity;
    let initial_history = last_history_bounds(&mut app);

    wheel(&mut app, MouseScrollUnit::Line, -1.);
    let line_history = last_history_bounds(&mut app);
    assert!(
        (initial_history.min.y - line_history.min.y - 48.).abs() < 1.,
        "one wheel line should move 32 logical pixels / 48 physical pixels at 150% DPI"
    );
    assert_eq!(measure_menu(&mut app, size).identity, header);

    wheel(&mut app, MouseScrollUnit::Line, 1.);
    assert_eq!(last_history_bounds(&mut app), initial_history);
    wheel(&mut app, MouseScrollUnit::Pixel, -32.);
    assert_eq!(last_history_bounds(&mut app), line_history);

    wheel(&mut app, MouseScrollUnit::Pixel, -10_000.);
    let history = last_history_bounds(&mut app);
    let viewport = body_bounds(&mut app);
    assert!(history.min.y >= viewport.min.y && history.max.y <= viewport.max.y);
    assert_eq!(measure_menu(&mut app, size).identity, header);
    wheel(&mut app, MouseScrollUnit::Pixel, 10_000.);
    assert_eq!(last_history_bounds(&mut app), initial_history);
    assert_identity_and_title_are_separate(&measure_menu(&mut app, size));
}

fn last_history_bounds(app: &mut App) -> Rect {
    let world = app.world_mut();
    world
        .query::<(&UiAction, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
        .find_map(|(action, node, transform)| {
            matches!(action, UiAction::JoinHistory(code) if code == "PCH-0000-0000-0000-0000")
                .then(|| rect(node, transform))
        })
        .expect("oldest recent lobby button")
}

fn body_bounds(app: &mut App) -> Rect {
    let world = app.world_mut();
    let (node, transform) = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<FrontendBody>>()
        .single(world)
        .unwrap();
    rect(node, transform)
}

#[test]
fn title_identity_overlap_page_scroll_reaches_history_without_moving_identity() {
    let size = UVec2::new(640, 600);
    let (mut app, _) = menu_app(size, 1., 4, "Alice", false);
    let initial = measure_menu(&mut app, size);
    let initial_history = last_history_bounds(&mut app);
    assert!(initial_history.max.y > body_bounds(&mut app).max.y);
    for _ in 0..3 {
        page(&mut app, KeyCode::PageDown);
    }
    let scrolled = measure_menu(&mut app, size);
    let history = last_history_bounds(&mut app);
    let viewport = body_bounds(&mut app);
    assert!(
        history.min.y >= viewport.min.y && history.max.y <= viewport.max.y,
        "history {history:?} must be visible inside scroll viewport {viewport:?}"
    );
    assert_eq!(scrolled.identity, initial.identity);
    assert_eq!(scrolled.previous, initial.previous);
    assert_eq!(scrolled.next, initial.next);
    for _ in 0..3 {
        page(&mut app, KeyCode::PageUp);
    }
    assert_eq!(last_history_bounds(&mut app), initial_history);
    assert_identity_and_title_are_separate(&measure_menu(&mut app, size));
}

//! A simple 3D scene showing how alpha blending can break and how order independent transparency (OIT) can fix it.
//!
//! See [`OrderIndependentTransparencyPlugin`] for the trade-offs of using OIT.
//!
//! [`OrderIndependentTransparencyPlugin`]: bevy::core_pipeline::oit::OrderIndependentTransparencyPlugin
use core::f32;

use bevy::{
    camera::visibility::RenderLayers,
    color::palettes::css::{BLUE, GREEN, RED, YELLOW},
    core_pipeline::{
        oit::{OitFragmentNode, OrderIndependentTransparencySettings},
        prepass::DepthPrepass,
    },
    prelude::*,
    render::render_resource::ShaderType,
    text::LineHeight,
    window::{PresentMode, WindowResized},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                toggle_oit,
                cycle_scenes,
                update_fragment_budget,
                update_max_fragments,
                update_threshold,
                update_animated,
                on_window_resize,
                animate_camera,
            ),
        )
        .run();
}

#[derive(Component)]
struct OitStatus;

#[derive(Component)]
struct FragmentBudget(f32);

#[derive(Component)]
struct MaxFragments(u32);

#[derive(Component)]
struct AlphaThreshold(f32);

#[derive(Component)]
struct BufferSize;

#[derive(Component)]
struct Animated;

fn format_buffer_size(width: f32, height: f32, avg_fragment_count: f32) -> String {
    let size = (width
        * height
        * (avg_fragment_count * OitFragmentNode::min_size().get() as f32 +
        /* heads */ 4.0)) as usize;
    let size_gib = size as f32 / (1 << 30) as f32;
    if size_gib >= 1.0 {
        format!("{:.1} GiB", size_gib)
    } else {
        let size_mib = size_gib * (1 << 10) as f32;
        if size_mib >= 1.0 {
            format!("{:.1} MiB", size_mib)
        } else {
            format!("{} kiB", (size_mib * (1 << 10) as f32) as usize)
        }
    }
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let oit_settings = OrderIndependentTransparencySettings::default();
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Add this component to this camera to render transparent meshes using OIT
        oit_settings.clone(),
        RenderLayers::layer(1),
        // Msaa currently doesn't work with OIT
        Msaa::Off,
        // Optional: depth prepass can help OIT filter out fragments occluded by opaque objects
        DepthPrepass,
    ));

    // light
    commands.spawn((
        PointLight {
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
        RenderLayers::layer(1),
    ));

    commands.spawn((
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            padding: UiRect::all(px(10)),
            ..default()
        },
        children![
            (Text::new("[Tab] Cycle scenes | [Space] Animate")),
            (
                Text::new("[A]dvanced transparency: "),
                children![(TextSpan::new("active"), TextColor(GREEN.into()))],
                OitStatus,
            ),
            (
                Text::new("Avg. [f]ragments per pixels: "),
                LineHeight::RelativeToFont(1.5),
                children![
                    (
                        TextSpan::new(format!("{:.2}", oit_settings.fragments_per_pixel_average)),
                        TextColor(RED.into())
                    ),
                    TextSpan::new("\n> buffer size: "),
                    (
                        TextSpan::new("-- MiB"),
                        TextFont {
                            weight: FontWeight::BOLD,
                            ..default()
                        },
                        BufferSize,
                    )
                ],
                FragmentBudget(oit_settings.fragments_per_pixel_average),
            ),
            (
                Text::new("Max [s]orted fragments: "),
                children![(
                    TextSpan::new(oit_settings.sorted_fragment_max_count.to_string()),
                    TextColor(RED.into())
                )],
                MaxFragments(oit_settings.sorted_fragment_max_count)
            ),
            (
                Text::new("Alpha [t]hreshold: "),
                children![(
                    TextSpan::new(format!("{:.2}", oit_settings.alpha_threshold)),
                    TextColor(RED.into())
                )],
                AlphaThreshold(oit_settings.alpha_threshold),
            )
        ],
    ));

    // spawn default scene
    spawn_spheres(&mut commands, &mut meshes, &mut materials);
}

fn on_window_resize(
    mut text: Single<&mut TextSpan, With<BufferSize>>,
    oit_settings: Single<&OrderIndependentTransparencySettings, With<Camera3d>>,
    mut resize_reader: MessageReader<WindowResized>,
) {
    for win in resize_reader.read() {
        text.0 = format_buffer_size(
            win.width,
            win.height,
            oit_settings.fragments_per_pixel_average,
        );
    }
}

fn update_threshold(
    mut text: Single<(Entity, &mut AlphaThreshold), With<Text>>,
    mut oit_settings: Single<&mut OrderIndependentTransparencySettings, With<Camera3d>>,
    mut text_writer: TextUiWriter,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyT) {
        let (e, ref mut alpha_threshold) = *text;
        let step = if keyboard_input.pressed(KeyCode::AltLeft) {
            0.01
        } else {
            0.1
        };
        if keyboard_input.pressed(KeyCode::ShiftLeft) {
            alpha_threshold.0 = 1f32.min(alpha_threshold.0 + step);
        } else {
            alpha_threshold.0 = 0f32.max(alpha_threshold.0 - step);
        };
        if alpha_threshold.0 != oit_settings.alpha_threshold {
            *text_writer.text(e, 1) = format!("{:.2}", alpha_threshold.0);
            oit_settings.alpha_threshold = alpha_threshold.0;
        }
    }
}

fn update_fragment_budget(
    mut text: Single<(Entity, &mut FragmentBudget), With<Text>>,
    window: Single<&Window>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut oit_settings: Single<&mut OrderIndependentTransparencySettings, With<Camera3d>>,
    mut text_writer: TextUiWriter,
) {
    if keyboard_input.just_pressed(KeyCode::KeyF) {
        let (e, ref mut fragment_count) = *text;
        let step = if keyboard_input.pressed(KeyCode::AltLeft) {
            0.25
        } else {
            1f32
        };
        if keyboard_input.pressed(KeyCode::ShiftLeft) {
            fragment_count.0 = 32f32.min(fragment_count.0 + step);
        } else {
            fragment_count.0 = 0f32.max(fragment_count.0 - step);
        };
        if fragment_count.0 != oit_settings.fragments_per_pixel_average {
            *text_writer.text(e, 1) = format!("{:.2}", fragment_count.0);
            oit_settings.fragments_per_pixel_average = fragment_count.0;
            *text_writer.text(e, 3) = format_buffer_size(
                window.width(),
                window.height(),
                oit_settings.fragments_per_pixel_average,
            );
        }
    }
}

fn update_max_fragments(
    mut text: Single<(Entity, &mut MaxFragments), With<Text>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut oit_settings: Single<&mut OrderIndependentTransparencySettings, With<Camera3d>>,
    mut text_writer: TextUiWriter,
) {
    if keyboard_input.just_pressed(KeyCode::KeyS) {
        let (e, ref mut max_fragments) = *text;
        let step = if keyboard_input.pressed(KeyCode::AltLeft) {
            1
        } else {
            8
        };
        if keyboard_input.pressed(KeyCode::ShiftLeft) {
            max_fragments.0 = 128u32.min(max_fragments.0 + step);
        } else {
            max_fragments.0 = 1u32.max(max_fragments.0.saturating_sub(step));
        };
        if max_fragments.0 != oit_settings.sorted_fragment_max_count {
            *text_writer.text(e, 1) = max_fragments.0.to_string();
            oit_settings.sorted_fragment_max_count = max_fragments.0;
        }
    }
}

fn toggle_oit(
    mut commands: Commands,
    fragment_budget: Single<&FragmentBudget>,
    max_fragments: Single<&MaxFragments>,
    alpha_threshold: Single<&AlphaThreshold>,
    text: Single<(Entity, &Children), With<OitStatus>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    q: Single<(Entity, Has<OrderIndependentTransparencySettings>), With<Camera3d>>,
    mut text_writer: TextUiWriter,
) {
    if keyboard_input.just_pressed(KeyCode::KeyA) {
        let (camera, has_oit) = *q;
        let (text, spans) = *text;
        *text_writer.text(text, 1) = if has_oit {
            // Removing the component will completely disable OIT for this camera
            commands
                .entity(camera)
                .remove::<OrderIndependentTransparencySettings>();
            commands
                .entity(*spans.get(0).unwrap())
                .insert(TextColor(RED.into()));
            "disabled".into()
        } else {
            // Adding the component to the camera will render any transparent meshes
            // with OIT instead of alpha blending
            commands
                .entity(camera)
                .insert(OrderIndependentTransparencySettings {
                    alpha_threshold: alpha_threshold.0,
                    sorted_fragment_max_count: max_fragments.0,
                    fragments_per_pixel_average: fragment_budget.0,
                });
            commands
                .entity(*spans.get(0).unwrap())
                .insert(TextColor(GREEN.into()));
            "enabled".into()
        };
    }
}

fn cycle_scenes(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<Entity, With<Mesh3d>>,
    mut scene_id: Local<usize>,
    asset_server: Res<AssetServer>,
) {
    if keyboard_input.just_pressed(KeyCode::Tab) {
        // despawn current scene
        for e in &q {
            commands.entity(e).despawn();
        }
        // increment scene_id
        *scene_id = (*scene_id + 1) % 4;
        // spawn next scene
        match *scene_id {
            0 => spawn_spheres(&mut commands, &mut meshes, &mut materials),
            1 => spawn_quads(&mut commands, &mut meshes, &mut materials),
            2 => spawn_occlusion_test(&mut commands, &mut meshes, &mut materials),
            3 => {
                spawn_auto_instancing_test(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    asset_server,
                );
            }
            _ => unreachable!(),
        }
    }
}

/// Spawns 3 overlapping spheres
/// Technically, when using `alpha_to_coverage` with MSAA this particular example wouldn't break,
/// but it breaks when disabling MSAA and is enough to show the difference between OIT enabled vs disabled.
fn spawn_spheres(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let pos_a = Vec3::new(-1.0, 0.75, 0.0);
    let pos_b = Vec3::new(0.0, -0.75, 0.0);
    let pos_c = Vec3::new(1.0, 0.75, 0.0);

    let offset = Vec3::new(0.0, 0.0, 0.0);

    let sphere_handle = meshes.add(Sphere::new(2.0).mesh());

    let alpha = 0.25;

    let render_layers = RenderLayers::layer(1);

    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED.with_alpha(alpha).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(pos_a + offset),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: GREEN.with_alpha(alpha).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(pos_b + offset),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BLUE.with_alpha(alpha).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(pos_c + offset),
        render_layers.clone(),
    ));
}

fn spawn_quads(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let quad_handle = meshes.add(Rectangle::new(3.0, 3.0).mesh());
    let render_layers = RenderLayers::layer(1);
    let xform = |x, y, z| {
        Transform::from_rotation(Quat::from_rotation_y(0.5))
            .mul_transform(Transform::from_xyz(x, y, z))
    };
    let common_params = StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    };
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED.with_alpha(0.5).into(),
            ..common_params.clone()
        })),
        xform(1.0, -0.1, 0.),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BLUE.with_alpha(0.8).into(),
            ..common_params.clone()
        })),
        xform(0.5, 0.2, -0.5),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: GREEN.with_green(1.0).with_alpha(0.5).into(),
            ..common_params.clone()
        })),
        xform(0.0, 0.4, -1.),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: YELLOW.with_alpha(0.3).into(),
            ..common_params.clone()
        })),
        xform(-0.5, 0.6, -1.1),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BLUE.with_alpha(0.2).into(),
            ..common_params.clone()
        })),
        xform(-0.8, 0.8, -1.2),
        render_layers.clone(),
    ));
}

/// Spawn a combination of opaque cubes and transparent spheres.
/// This is useful to make sure transparent meshes drawn with OIT
/// are properly occluded by opaque meshes.
fn spawn_occlusion_test(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let sphere_handle = meshes.add(Sphere::new(1.0).mesh());
    let cube_handle = meshes.add(Cuboid::from_size(Vec3::ONE).mesh());
    let cube_material = materials.add(Color::srgb(0.8, 0.7, 0.6));

    let render_layers = RenderLayers::layer(1);

    // front
    let x = -2.5;
    commands.spawn((
        Mesh3d(cube_handle.clone()),
        MeshMaterial3d(cube_material.clone()),
        Transform::from_xyz(x, 0.0, 2.0),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED.with_alpha(0.5).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(x, 0., 0.),
        render_layers.clone(),
    ));

    // intersection
    commands.spawn((
        Mesh3d(cube_handle.clone()),
        MeshMaterial3d(cube_material.clone()),
        Transform::from_xyz(x, 0.0, 1.0),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED.with_alpha(0.5).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(0., 0., 0.),
        render_layers.clone(),
    ));

    // back
    let x = 2.5;
    commands.spawn((
        Mesh3d(cube_handle.clone()),
        MeshMaterial3d(cube_material.clone()),
        Transform::from_xyz(x, 0.0, -2.0),
        render_layers.clone(),
    ));
    commands.spawn((
        Mesh3d(sphere_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED.with_alpha(0.5).into(),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_xyz(x, 0., 0.),
        render_layers.clone(),
    ));
}

fn spawn_auto_instancing_test(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: Res<AssetServer>,
) {
    let render_layers = RenderLayers::layer(1);

    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material_handle = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        base_color_texture: Some(asset_server.load("textures/slice_square.png")),
        ..Default::default()
    });
    let mut bundles = Vec::with_capacity(3 * 3 * 3);

    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                bundles.push((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(material_handle.clone()),
                    Transform::from_xyz(x as f32 * 2.0, y as f32 * 2.0, z as f32 * 2.0),
                    render_layers.clone(),
                ));
            }
        }
    }
    commands.spawn_batch(bundles);
}

fn update_animated(
    mut commands: Commands,
    camera: Single<(Entity, Has<Animated>), With<Camera3d>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        let (camera, animated) = *camera;
        if animated {
            commands.entity(camera).remove::<Animated>();
        } else {
            commands.entity(camera).insert(Animated);
        }
    }
}

fn animate_camera(
    mut transform: Single<&mut Transform, (With<Camera3d>, With<Animated>)>,
    time: Res<Time>,
) {
    const RADIANS_PER_SECS: f32 = f32::consts::PI / 2.0;
    let angle = time.delta_secs() * RADIANS_PER_SECS;
    transform.rotate_around(Vec3::ZERO, Quat::from_rotation_y(angle));
}

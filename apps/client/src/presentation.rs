use bevy::{
    camera::{ScalingMode, Viewport},
    prelude::*,
    transform::{TransformSystems, helper::TransformHelper},
    window::PrimaryWindow,
};
use std::collections::BTreeMap;
#[cfg(feature = "dev")]
use std::{path::Path, time::SystemTime};
#[cfg(feature = "dev")]
use world01_configs::load_file;
use world01_network::{
    Client, ClientPositionCorrection, RemotePositionExtrapolation, connect_client,
};
use world01_world_data::{
    Ankh, AnkhLayout, CharacterHealth, CharacterId, CharacterLifeState, GazeDirection,
    MovementIntent, Position, RunState, SelectedCharacter, WorldMap,
};

use crate::eyes::EyePupil;
use crate::hammer::apply_hammer_pose;
use crate::input::{
    ClientInputFocus, clear_input_when_unfocused, collect_attack_input,
    collect_death_confirmation_input, collect_gaze_input, collect_locomotion_input,
    collect_movement_input, update_client_input_focus,
};
use crate::polytools::{
    CharacterAssetLibrary, bevy_pupil_mesh, repeated_flat_asset_mesh, spawn_character_visual,
    spawn_projected_prop_visual,
};
use crate::pose::{
    PoseSettings, apply_body_facing, apply_character_status_presentation, apply_neutral_head_motion,
};
use crate::projection::ProjectionDepthMaterial;
use crate::session::{ClientScreen, ClientSession};

const VIEWPORT_WIDTH_METERS: f32 = 15.0;
const VIEWPORT_HEIGHT_METERS: f32 = 9.375;
const SELECTION_HEIGHT_METERS: f32 = VIEWPORT_HEIGHT_METERS * 0.75;
const PREVIEW_SCALE: f32 = 0.95;
const CORRECTION_HALF_LIFE_SECONDS: f32 = 0.2;
const CORRECTION_EPSILON_SQUARED: f32 = 0.000_001;
const TERRAIN_PRESENTATION_LAYER: f32 = -10.0;
const PROP_PRESENTATION_LAYER: f32 = -1.0;
const ANKH_PRESENTATION_LAYER: f32 = -1.0;
const ANKH_TILT_DEGREES: f32 = 30.0;
const ANKH_OUTLINE_DEPTH_METERS: f32 = 0.025;

pub struct ClientPresentationPlugin {
    pub character_assets: CharacterAssetLibrary,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraView {
    pub width_tiles: u32,
    pub height_tiles: u32,
}

impl CameraView {
    pub const fn new(width_tiles: u32, height_tiles: u32) -> Self {
        Self {
            width_tiles,
            height_tiles,
        }
    }

    pub fn width_meters(self) -> f32 {
        self.width_tiles as f32
    }
    pub fn height_meters(self) -> f32 {
        self.height_tiles as f32
    }
}

impl Plugin for ClientPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.character_assets.clone())
            .init_resource::<ClientInputFocus>()
            .init_resource::<PoseSettings>()
            .add_systems(OnEnter(ClientScreen::CharacterSelection), setup_selection)
            .add_systems(
                OnEnter(ClientScreen::InGame),
                (
                    configure_ingame_camera,
                    setup_map_visuals,
                    setup_ankh_visuals,
                ),
            )
            .add_systems(OnExit(ClientScreen::CharacterSelection), cleanup_selection)
            .add_systems(OnExit(ClientScreen::InGame), cleanup_room_floor)
            .add_systems(
                Update,
                (
                    (configure_ingame_camera, apply_letterbox_viewport)
                        .run_if(in_state(ClientScreen::InGame)),
                    (handle_selection_input, update_selection_feedback)
                        .chain()
                        .run_if(in_state(ClientScreen::CharacterSelection)),
                    (
                        update_client_input_focus,
                        clear_input_when_unfocused,
                        collect_movement_input,
                        collect_gaze_input,
                        collect_attack_input,
                        collect_locomotion_input,
                        collect_death_confirmation_input,
                    )
                        .chain(),
                    (apply_body_facing, apply_neutral_head_motion),
                    (render_new_players, initialize_local_render_history)
                        .chain()
                        .run_if(in_state(ClientScreen::InGame)),
                    repick_character.run_if(in_state(ClientScreen::InGame)),
                ),
            )
            .add_systems(
                FixedPostUpdate,
                capture_local_render_positions.run_if(in_state(ClientScreen::InGame)),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_rendered_positions,
                    apply_run_motion,
                    update_health_bars,
                    follow_local_character,
                    apply_eye_gaze,
                    apply_character_status_presentation,
                    apply_hammer_pose,
                )
                    .chain()
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(ClientScreen::InGame)),
            );
        #[cfg(feature = "dev")]
        app.add_systems(
            Update,
            hot_reload_design
                .before(configure_ingame_camera)
                .run_if(in_state(ClientScreen::InGame)),
        );
    }
}

#[derive(Component)]
struct RenderedMap;

#[derive(Component)]
struct RenderedAnkh;

#[derive(Component)]
struct SelectionVisual;

#[derive(Component)]
struct SelectionPreview(CharacterId);

#[derive(Component)]
struct SelectionButton(CharacterId);

#[derive(Component)]
struct ConfirmButton;

#[derive(Component)]
struct ConfirmButtonLabel;

#[derive(Component)]
struct RenderedCharacter;

#[derive(Component)]
struct HealthBarFill(Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct BodyPivot(Vec2);

#[derive(Component)]
struct PresentationCamera;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct LocalRenderHistory {
    previous: Vec2,
    current: Vec2,
}

fn setup_selection(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut projection_materials: ResMut<Assets<ProjectionDepthMaterial>>,
    character_assets: Res<CharacterAssetLibrary>,
    cameras: Query<(), With<PresentationCamera>>,
) {
    if cameras.is_empty() {
        commands.spawn((
            Camera2d,
            PresentationCamera,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: VIEWPORT_HEIGHT_METERS,
                },
                ..OrthographicProjection::default_2d()
            }),
        ));
    }

    let selection_center_y = (VIEWPORT_HEIGHT_METERS - SELECTION_HEIGHT_METERS) * 0.5;
    let character_ids = character_assets.ids();
    let panel_width = VIEWPORT_WIDTH_METERS / character_ids.len().max(1) as f32;
    for (index, character) in character_ids.iter().cloned().enumerate() {
        let x = -VIEWPORT_WIDTH_METERS * 0.5 + panel_width * (index as f32 + 0.5);
        let root = commands
            .spawn((
                Transform::from_xyz(x, selection_center_y + 0.15, 0.0)
                    .with_scale(Vec3::splat(PREVIEW_SCALE)),
                Visibility::default(),
                SelectionVisual,
                SelectionPreview(character.clone()),
            ))
            .id();
        if let Err(error) = spawn_character_visual(
            &mut commands,
            root,
            &mut meshes,
            &mut materials,
            &mut projection_materials,
            &character_assets,
            &character,
        ) {
            error!("cannot spawn PolyTools selection visual: {error}");
        }
    }

    let panel = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            SelectionVisual,
        ))
        .id();
    let row = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(75),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            SelectionVisual,
        ))
        .id();
    let button_width = 100.0 / character_ids.len().max(1) as f32;
    for character in character_ids {
        let button = commands
            .spawn((selection_button(character, button_width), SelectionVisual))
            .id();
        commands.entity(row).add_child(button);
    }
    let footer = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(25),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            SelectionVisual,
            children![confirm_button()],
        ))
        .id();
    commands.entity(panel).add_child(row);
    commands.entity(panel).add_child(footer);
}

fn apply_letterbox_viewport(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<PresentationCamera>>,
    camera_view: Res<CameraView>,
    screen: Res<State<ClientScreen>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    let aspect = if screen.get() == &ClientScreen::InGame {
        camera_view.width_meters() / camera_view.height_meters()
    } else {
        VIEWPORT_WIDTH_METERS / VIEWPORT_HEIGHT_METERS
    };
    camera.viewport = Some(letterbox_viewport(window.physical_size(), aspect));
}

#[cfg(feature = "dev")]
fn hot_reload_design(
    mut last_modified: Local<Option<SystemTime>>,
    mut camera_view: ResMut<CameraView>,
) {
    let path = Path::new("crates/configs/design.toml");
    let Ok(modified) = std::fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return;
    };
    let Some(previous_modified) = *last_modified else {
        *last_modified = Some(modified);
        return;
    };
    if previous_modified == modified {
        return;
    }
    *last_modified = Some(modified);

    let Ok(design) = load_file(path) else {
        warn!("ignoring invalid hot-reloaded design configuration");
        return;
    };
    let Some((camera_width, camera_height)) = design.camera.effective_view_tiles() else {
        warn!("ignoring hot-reloaded configuration with invalid camera dimensions");
        return;
    };
    *camera_view = CameraView::new(camera_width, camera_height);
    info!("reloaded camera configuration");
}

fn configure_ingame_camera(
    camera_view: Res<CameraView>,
    mut cameras: Query<&mut Projection, With<PresentationCamera>>,
) {
    if !camera_view.is_changed() {
        return;
    }

    for mut projection in &mut cameras {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scaling_mode = ScalingMode::FixedVertical {
                viewport_height: camera_view.height_meters(),
            };
        }
    }
}

fn letterbox_viewport(window_size: UVec2, target_aspect: f32) -> Viewport {
    if window_size.x == 0 || window_size.y == 0 {
        return Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::ZERO,
            depth: 0.0..1.0,
        };
    }

    let width_from_height = (window_size.y as f32 * target_aspect).round() as u32;
    let viewport_size = if width_from_height <= window_size.x {
        UVec2::new(width_from_height, window_size.y)
    } else {
        UVec2::new(
            window_size.x,
            (window_size.x as f32 / target_aspect).round() as u32,
        )
    };

    Viewport {
        physical_position: (window_size - viewport_size) / 2,
        physical_size: viewport_size,
        depth: 0.0..1.0,
    }
}

fn selection_button(character: CharacterId, width_percent: f32) -> impl Bundle {
    (
        Button,
        SelectionButton(character.clone()),
        Node {
            width: percent(width_percent),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexEnd,
            padding: UiRect::new(px(8), px(8), px(8), px(18)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.85, 0.85, 0.9, 0.35)),
        BackgroundColor(panel_color(&character, false).with_alpha(0.30)),
        children![(
            Text::new(character.label()),
            TextFont::from_font_size(20.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Node {
                width: percent(100),
                ..default()
            }
        )],
    )
}

fn confirm_button() -> impl Bundle {
    (
        Button,
        ConfirmButton,
        Node {
            width: percent(42),
            height: percent(48),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.85, 0.72, 0.45, 0.45)),
        BackgroundColor(Color::srgba(0.18, 0.19, 0.22, 0.80)),
        children![(
            ConfirmButtonLabel,
            Text::new("SELECT CHARACTER & JOIN"),
            TextFont::from_font_size(26.0),
            TextColor(Color::srgba(0.75, 0.75, 0.78, 1.0))
        )],
    )
}

fn handle_selection_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<ClientSession>,
    mut next_screen: ResMut<NextState<ClientScreen>>,
    buttons: Query<
        (
            &Interaction,
            Option<&SelectionButton>,
            Option<&ConfirmButton>,
        ),
        With<Button>,
    >,
    mut commands: Commands,
    character_assets: Res<CharacterAssetLibrary>,
) -> Result {
    if session.joining {
        return Ok(());
    }

    for (interaction, selection_button, confirm_button) in &buttons {
        if *interaction == Interaction::Pressed {
            if let Some(selection_button) = selection_button {
                session.selected = Some(selection_button.0.clone());
            } else if confirm_button.is_some() {
                return join_selected_character(&mut session, &mut commands, &mut next_screen);
            }
        }
    }

    for (index, character) in character_assets.ids().into_iter().enumerate() {
        let key = match index {
            0 => KeyCode::Digit1,
            1 => KeyCode::Digit2,
            2 => KeyCode::Digit3,
            3 => KeyCode::Digit4,
            4 => KeyCode::Digit5,
            5 => KeyCode::Digit6,
            6 => KeyCode::Digit7,
            7 => KeyCode::Digit8,
            _ => continue,
        };
        if keyboard.just_pressed(key) {
            session.selected = Some(character);
        }
    }

    if keyboard.just_pressed(KeyCode::ArrowLeft) {
        session.selected =
            adjacent_character(session.selected.as_ref(), -1, &character_assets.ids());
    } else if keyboard.just_pressed(KeyCode::ArrowRight) {
        session.selected =
            adjacent_character(session.selected.as_ref(), 1, &character_assets.ids());
    }

    if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter) {
        return join_selected_character(&mut session, &mut commands, &mut next_screen);
    }

    Ok(())
}

fn update_selection_feedback(
    session: Res<ClientSession>,
    mut buttons: Query<
        (
            &Interaction,
            Option<&SelectionButton>,
            Option<&ConfirmButton>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
    mut confirm_label: Query<&mut TextColor, With<ConfirmButtonLabel>>,
    mut previews: Query<(&SelectionPreview, &mut Transform)>,
) {
    let mut hovered_character = None;
    for (interaction, selection_button, confirm_button, mut background, mut border) in &mut buttons
    {
        if let Some(selection_button) = selection_button {
            let selected = session.selected.as_ref() == Some(&selection_button.0);
            if *interaction == Interaction::Hovered {
                hovered_character = Some(selection_button.0.clone());
            }
            let intensity = match (*interaction, selected) {
                (Interaction::Pressed, _) => 0.72,
                (_, true) => 0.62,
                (Interaction::Hovered, false) => 0.48,
                _ => 0.30,
            };
            **background = panel_color(&selection_button.0, selected || intensity > 0.30)
                .with_alpha(intensity);
            let border_color = if selected {
                Color::srgba(0.96, 0.79, 0.38, 0.95)
            } else if *interaction == Interaction::Hovered {
                Color::srgba(0.92, 0.92, 0.96, 0.70)
            } else {
                Color::srgba(0.85, 0.85, 0.90, 0.35)
            };
            *border = BorderColor::all(border_color);
        } else if confirm_button.is_some() {
            let enabled = session.selected.is_some();
            **background = match (*interaction, enabled) {
                (Interaction::Pressed, true) => Color::srgba(0.55, 0.35, 0.13, 1.0),
                (Interaction::Hovered, true) => Color::srgba(0.46, 0.30, 0.13, 0.96),
                (_, true) => Color::srgba(0.38, 0.25, 0.12, 0.92),
                _ => Color::srgba(0.18, 0.19, 0.22, 0.62),
            };
            let border_color = if enabled {
                Color::srgba(0.92, 0.73, 0.34, 0.85)
            } else {
                Color::srgba(0.55, 0.55, 0.58, 0.30)
            };
            *border = BorderColor::all(border_color);
        }
    }

    if let Ok(mut color) = confirm_label.single_mut() {
        **color = if session.selected.is_some() {
            Color::WHITE
        } else {
            Color::srgba(0.55, 0.55, 0.58, 1.0)
        };
    }

    for (preview, mut transform) in &mut previews {
        let scale = if session.selected.as_ref() == Some(&preview.0) {
            PREVIEW_SCALE * 1.08
        } else if hovered_character.as_ref() == Some(&preview.0) {
            PREVIEW_SCALE * 1.04
        } else {
            PREVIEW_SCALE
        };
        transform.scale = Vec3::splat(scale);
    }
}

fn adjacent_character(
    selected: Option<&CharacterId>,
    offset: isize,
    ids: &[CharacterId],
) -> Option<CharacterId> {
    if ids.is_empty() {
        return None;
    }
    let Some(current) =
        selected.and_then(|selected| ids.iter().position(|candidate| candidate == selected))
    else {
        return Some(ids[if offset < 0 { ids.len() - 1 } else { 0 }].clone());
    };
    let next = (current as isize + offset).rem_euclid(ids.len() as isize) as usize;
    Some(ids[next].clone())
}

fn join_selected_character(
    session: &mut ClientSession,
    commands: &mut Commands,
    next_screen: &mut NextState<ClientScreen>,
) -> Result {
    let Some(character) = session.selected.clone() else {
        return Ok(());
    };

    connect_client(
        commands,
        session.client_id,
        character,
        session.remote_interpolation_ratio,
        session.network_simulation,
    )?;
    session.joining = true;
    next_screen.set(ClientScreen::InGame);
    Ok(())
}

fn cleanup_selection(
    selection_visuals: Query<(Entity, Option<&ChildOf>), With<SelectionVisual>>,
    mut commands: Commands,
) {
    for (entity, parent) in &selection_visuals {
        if parent.is_none() {
            commands.entity(entity).despawn();
        }
    }
}

fn setup_ankh_visuals(
    mut commands: Commands,
    layout: Res<AnkhLayout>,
    character_assets: Res<CharacterAssetLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ProjectionDepthMaterial>>,
) {
    let Some(ankh_manifest) = character_assets.prop("ankh") else {
        error!("cannot spawn Ankhs: missing Ankh prop manifest");
        return;
    };

    for (index, position) in layout.positions.iter().copied().enumerate() {
        let root = commands
            .spawn((
                Ankh::new(index as u32),
                RenderedAnkh,
                position,
                Transform::from_xyz(position.x, position.y, ANKH_PRESENTATION_LAYER),
                Visibility::default(),
            ))
            .id();
        if let Err(error) = spawn_projected_prop_visual(
            &mut commands,
            root,
            &mut meshes,
            &mut materials,
            ankh_manifest,
            Color::srgb(0.72, 0.56, 0.20),
            ankh_projection_rotation(),
            ANKH_PRESENTATION_LAYER,
            ANKH_OUTLINE_DEPTH_METERS,
        ) {
            error!("cannot spawn Ankh prop visual: {error}");
        }
    }
}

fn setup_map_visuals(
    mut commands: Commands,
    map: Res<WorldMap>,
    character_assets: Res<CharacterAssetLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut flat_materials: ResMut<Assets<ColorMaterial>>,
    mut projection_materials: ResMut<Assets<ProjectionDepthMaterial>>,
) {
    let mut terrain_offsets = BTreeMap::<&str, Vec<Vec2>>::new();
    for cell in map.terrain_cells() {
        terrain_offsets
            .entry(&cell.asset_key)
            .or_default()
            .push(Vec2::new(cell.center.x, cell.center.y));
    }
    for (asset_key, offsets) in terrain_offsets {
        let Some(manifest) = character_assets.terrain(asset_key) else {
            error!(
                asset_key,
                "cannot render map terrain: missing PolyTools manifest"
            );
            continue;
        };
        match repeated_flat_asset_mesh(manifest, offsets) {
            Ok(mesh) => {
                commands.spawn((
                    RenderedMap,
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(flat_materials.add(map_asset_color(asset_key))),
                    Transform::from_xyz(0.0, 0.0, TERRAIN_PRESENTATION_LAYER),
                ));
            }
            Err(error) => error!(asset_key, %error, "cannot build repeated map terrain mesh"),
        }
    }

    for placement in map
        .placements()
        .iter()
        .chain(map.transitions().iter())
        .filter(|placement| placement.asset_key != "ankh")
    {
        let Some(manifest) = character_assets.prop(&placement.asset_key) else {
            error!(asset_key = %placement.asset_key, "cannot render map prop: missing PolyTools manifest");
            continue;
        };
        let root = commands
            .spawn((
                RenderedMap,
                Transform::from_xyz(
                    placement.position.x,
                    placement.position.y,
                    PROP_PRESENTATION_LAYER,
                ),
                Visibility::default(),
            ))
            .id();
        if let Err(error) = spawn_projected_prop_visual(
            &mut commands,
            root,
            &mut meshes,
            &mut projection_materials,
            manifest,
            map_asset_color(&placement.asset_key),
            Quat::IDENTITY,
            PROP_PRESENTATION_LAYER,
            ANKH_OUTLINE_DEPTH_METERS,
        ) {
            error!(asset_key = %placement.asset_key, %error, "cannot spawn map prop visual");
        }
    }
}

fn map_asset_color(asset_key: &str) -> Color {
    match asset_key {
        "grass" => Color::srgb(0.34, 0.62, 0.22),
        "tree" => Color::srgb(0.18, 0.46, 0.14),
        _ => Color::srgb(0.45, 0.45, 0.48),
    }
}

fn ankh_projection_rotation() -> Quat {
    Quat::from_rotation_x(ANKH_TILT_DEGREES.to_radians())
        * Quat::from_rotation_y(ANKH_TILT_DEGREES.to_radians())
}

fn cleanup_room_floor(
    map_visuals: Query<Entity, With<RenderedMap>>,
    ankhs: Query<Entity, With<RenderedAnkh>>,
    mut commands: Commands,
) {
    for entity in &map_visuals {
        commands.entity(entity).despawn();
    }
    for entity in &ankhs {
        commands.entity(entity).despawn();
    }
}

fn repick_character(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<ClientSession>,
    mut next_screen: ResMut<NextState<ClientScreen>>,
    clients: Query<Entity, With<Client>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyR) {
        return;
    }
    for client in &clients {
        commands.entity(client).despawn();
    }
    session.selected = None;
    session.joining = false;
    next_screen.set(ClientScreen::CharacterSelection);
}

fn render_new_players(
    mut commands: Commands,
    players: Query<(Entity, &SelectedCharacter, &Position), Without<RenderedCharacter>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut projection_materials: ResMut<Assets<ProjectionDepthMaterial>>,
    character_assets: Res<CharacterAssetLibrary>,
) {
    for (entity, character, position) in &players {
        commands.entity(entity).insert((
            RenderedCharacter,
            BodyPivot(character_assets.body_pivot(&character.0)),
            Visibility::default(),
            Transform::from_xyz(position.x, position.y, 0.0),
        ));
        let bar = commands
            .spawn((
                Sprite::from_color(Color::srgb(0.85, 0.12, 0.08), Vec2::ONE),
                Transform::from_xyz(
                    -0.4,
                    character_assets.health_bar_offset_y(&character.0),
                    20.0,
                )
                .with_scale(Vec3::new(0.8, 0.06, 1.0)),
                HealthBarFill(entity),
            ))
            .id();
        commands.entity(entity).add_child(bar);
        if let Err(error) = spawn_character_visual(
            &mut commands,
            entity,
            &mut meshes,
            &mut materials,
            &mut projection_materials,
            &character_assets,
            &character.0,
        ) {
            error!("cannot spawn PolyTools player visual: {error}");
        }
    }
}

fn update_health_bars(
    health: Query<&CharacterHealth>,
    mut bars: Query<(&HealthBarFill, &mut Transform)>,
) {
    for (HealthBarFill(owner), mut transform) in &mut bars {
        let Ok(value) = health.get(*owner) else {
            continue;
        };
        let ratio = if value.maximum > 0.0 {
            (value.current / value.maximum).clamp(0.0, 1.0)
        } else {
            0.0
        };
        transform.translation.x = -0.4 * (1.0 - ratio);
        transform.scale.x = 0.8 * ratio;
    }
}

fn initialize_local_render_history(
    mut commands: Commands,
    players: Query<
        (Entity, &Position),
        (
            With<RenderedCharacter>,
            With<MovementIntent>,
            Without<LocalRenderHistory>,
        ),
    >,
) {
    for (entity, position) in &players {
        let current = Vec2::new(position.x, position.y);
        commands.entity(entity).insert(LocalRenderHistory {
            previous: current,
            current,
        });
    }
}

fn capture_local_render_positions(
    mut players: Query<(&Position, &mut LocalRenderHistory), With<RenderedCharacter>>,
) {
    for (position, mut history) in &mut players {
        history.previous = history.current;
        history.current = Vec2::new(position.x, position.y);
    }
}

fn sync_rendered_positions(
    fixed_time: Res<Time<Fixed>>,
    virtual_time: Res<Time<Virtual>>,
    mut players: Query<
        (
            Entity,
            &Position,
            Option<&LocalRenderHistory>,
            Option<&RemotePositionExtrapolation>,
            Option<&mut ClientPositionCorrection>,
            Option<&CharacterLifeState>,
            &mut Transform,
        ),
        With<RenderedCharacter>,
    >,
    mut commands: Commands,
) {
    let alpha = fixed_time.overstep_fraction();
    let correction_decay = correction_decay(virtual_time.delta_secs());

    for (entity, position, history, extrapolation, correction, life, mut transform) in &mut players
    {
        let mut rendered = sampled_render_position(*position, history, alpha);
        if history.is_none()
            && life.is_none_or(|life| life.is_alive())
            && let Some(extrapolation) = extrapolation
        {
            rendered += extrapolation.offset;
        }

        if let Some(mut correction) = correction {
            if correction.is_changed() {
                correction.offset =
                    Vec2::new(transform.translation.x, transform.translation.y) - rendered;
            }
            correction.offset *= correction_decay;
            rendered += correction.offset;
            if correction.offset.length_squared() <= CORRECTION_EPSILON_SQUARED {
                commands.entity(entity).remove::<ClientPositionCorrection>();
            }
        }

        transform.translation.x = rendered.x;
        transform.translation.y = rendered.y;
    }
}

const RUN_ROCK_AMPLITUDE_RADIANS: f32 = 0.08;
const RUN_ROCK_FREQUENCY_HZ: f32 = 8.0;

fn apply_run_motion(
    virtual_time: Res<Time<Virtual>>,
    mut players: Query<(&RunState, &mut Transform), With<RenderedCharacter>>,
) {
    let phase = virtual_time.elapsed_secs() * std::f32::consts::TAU * RUN_ROCK_FREQUENCY_HZ;
    let rock = phase.sin() * RUN_ROCK_AMPLITUDE_RADIANS;

    for (run, mut transform) in &mut players {
        transform.rotation = if run.active {
            Quat::from_rotation_z(rock)
        } else {
            Quat::IDENTITY
        };
    }
}

fn follow_local_character(
    local_players: Query<(&Transform, &BodyPivot), (With<RenderedCharacter>, With<MovementIntent>)>,
    mut cameras: Query<&mut Transform, (With<PresentationCamera>, Without<RenderedCharacter>)>,
) {
    let Ok((player_transform, body_pivot)) = local_players.single() else {
        return;
    };
    let Ok(mut camera_transform) = cameras.single_mut() else {
        return;
    };

    let anchor = player_transform.translation.truncate() + body_pivot.0;
    camera_transform.translation.x = anchor.x;
    camera_transform.translation.y = anchor.y;
}

fn apply_eye_gaze(
    players: Query<&GazeDirection>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pupils: ParamSet<(
        TransformHelper,
        Query<(Entity, &EyePupil, &Mesh2d, &mut Transform)>,
    )>,
) {
    let pupil_owners = {
        let mut pupil_query = pupils.p1();
        pupil_query
            .iter_mut()
            .map(|(entity, pupil, _, _)| (entity, pupil.owner))
            .collect::<Vec<_>>()
    };

    for (entity, owner) in pupil_owners {
        let Ok(gaze) = players.get(owner) else {
            continue;
        };
        let Ok(global_transform) = pupils.p0().compute_global_transform(entity) else {
            warn!(?entity, "cannot compute current eye transform");
            continue;
        };
        let mut pupil_query = pupils.p1();
        let Ok((_, pupil, mesh_handle, mut transform)) = pupil_query.get_mut(entity) else {
            continue;
        };
        let position = pupil.position_for_world_gaze(Vec2::new(gaze.x, gaze.y), &global_transform);
        if transform.translation.truncate().distance_squared(position) <= f32::EPSILON {
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else {
            warn!("cannot update clipped pupil mesh");
            continue;
        };
        *mesh = bevy_pupil_mesh(&pupil.clipped_geometry(position));
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}

fn sampled_render_position(
    position: Position,
    history: Option<&LocalRenderHistory>,
    alpha: f32,
) -> Vec2 {
    history.map_or_else(
        || Vec2::new(position.x, position.y),
        |history| history.previous.lerp(history.current, alpha),
    )
}

fn correction_decay(delta_seconds: f32) -> f32 {
    0.5_f32.powf(delta_seconds / CORRECTION_HALF_LIFE_SECONDS)
}

fn panel_color(character: &CharacterId, selected: bool) -> Color {
    let base = match character.0.as_str() {
        "wizard" => (0.16, 0.12, 0.28),
        "mage" => (0.10, 0.24, 0.27),
        "sorcerer" => (0.29, 0.10, 0.12),
        "rogue" => (0.12, 0.13, 0.15),
        "glavier" => (0.27, 0.20, 0.08),
        "barde" => (0.28, 0.15, 0.08),
        "chantres" => (0.22, 0.10, 0.28),
        "hammerer" => (0.24, 0.16, 0.10),
        "monk" => (0.27, 0.15, 0.05),
        _ => (0.16, 0.18, 0.22),
    };
    let multiplier = if selected { 1.65 } else { 1.0 };
    Color::srgb(
        base.0 * multiplier,
        base.1 * multiplier,
        base.2 * multiplier,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_selection_wraps_in_both_directions() {
        let ids = vec![
            CharacterId::new("glavier").unwrap(),
            CharacterId::new("wizard").unwrap(),
        ];
        assert_eq!(
            adjacent_character(Some(&ids[0]), -1, &ids),
            Some(ids[1].clone())
        );
        assert_eq!(
            adjacent_character(Some(&ids[1]), 1, &ids),
            Some(ids[0].clone())
        );
    }

    #[test]
    fn keyboard_selection_starts_at_directional_edge() {
        let ids = vec![
            CharacterId::new("glavier").unwrap(),
            CharacterId::new("wizard").unwrap(),
        ];
        assert_eq!(adjacent_character(None, 1, &ids), Some(ids[0].clone()));
        assert_eq!(adjacent_character(None, -1, &ids), Some(ids[1].clone()));
    }

    #[test]
    fn letterbox_keeps_the_full_16_by_10_viewport_inside_widescreen() {
        let viewport = letterbox_viewport(UVec2::new(1920, 1080), 16.0 / 10.0);
        assert_eq!(viewport.physical_position, UVec2::new(96, 0));
        assert_eq!(viewport.physical_size, UVec2::new(1728, 1080));
    }

    #[test]
    fn letterbox_keeps_the_full_16_by_10_viewport_inside_tall_windows() {
        let viewport = letterbox_viewport(UVec2::new(1200, 1200), 16.0 / 10.0);
        assert_eq!(viewport.physical_position, UVec2::new(0, 225));
        assert_eq!(viewport.physical_size, UVec2::new(1200, 750));
    }

    #[test]
    fn ankh_projection_applies_positive_y_then_x_tilts() {
        let rotation = ankh_projection_rotation();
        let projected_up = rotation * Vec3::Y;
        let projected_right = rotation * Vec3::X;
        let radians = ANKH_TILT_DEGREES.to_radians();
        let expected_up = Vec3::new(0.0, radians.cos(), radians.sin());
        let expected_right = Vec3::new(
            radians.cos(),
            radians.sin() * radians.sin(),
            -radians.sin() * radians.cos(),
        );

        assert!(projected_up.distance(expected_up) < 0.000_001);
        assert!(projected_right.distance(expected_right) < 0.000_001);
        assert!((rotation.length() - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn presentation_sync_only_derives_translation_from_position() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PostUpdate, sync_rendered_positions);
        let entity = app
            .world_mut()
            .spawn((
                Position::new(2.5, -1.25),
                Transform::from_xyz(9.0, 8.0, 3.0).with_scale(Vec3::splat(1.5)),
                RenderedCharacter,
            ))
            .id();

        app.update();

        let transform = app
            .world()
            .get::<Transform>(entity)
            .expect("rendered entity retains its presentation Transform");
        assert_eq!(transform.translation, Vec3::new(2.5, -1.25, 3.0));
        assert_eq!(transform.scale, Vec3::splat(1.5));
    }

    #[test]
    fn camera_follows_the_local_character() {
        let mut app = App::new();
        app.add_systems(Update, follow_local_character);
        let camera = app
            .world_mut()
            .spawn((PresentationCamera, Transform::default()))
            .id();
        app.world_mut().spawn((
            RenderedCharacter,
            MovementIntent::ZERO,
            BodyPivot(Vec2::new(1.0, 2.0)),
            Transform::from_xyz(3.0, -4.0, 0.0),
        ));

        app.update();

        assert_eq!(
            app.world()
                .get::<Transform>(camera)
                .map(|transform| transform.translation),
            Some(Vec3::new(4.0, -2.0, 0.0))
        );
    }

    #[test]
    fn local_render_position_samples_fixed_tick_history() {
        let history = LocalRenderHistory {
            previous: Vec2::new(1.0, -2.0),
            current: Vec2::new(5.0, 2.0),
        };

        assert_eq!(
            sampled_render_position(Position::new(99.0, 99.0), Some(&history), 0.25),
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn remote_render_position_uses_snapshot_interpolated_value_directly() {
        assert_eq!(
            sampled_render_position(Position::new(3.0, -4.0), None, 0.25),
            Vec2::new(3.0, -4.0)
        );
    }

    #[test]
    fn reconciliation_error_halves_over_configured_period() {
        assert!((correction_decay(CORRECTION_HALF_LIFE_SECONDS) - 0.5).abs() < f32::EPSILON);
    }
}

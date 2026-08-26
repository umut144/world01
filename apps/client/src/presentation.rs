use bevy::{
    camera::{ScalingMode, Viewport},
    prelude::*,
    window::PrimaryWindow,
};
use game01_network::{
    ClientMovementInput, ClientPositionCorrection, NetworkSimulationProfile,
    RemotePositionExtrapolation, configure_client, connect_client,
};
use game01_world_data::{
    CharacterKind, MovementIntent, Position, RoomId, SelectedCharacter, StartingRoomGrid,
};

use crate::controller::ControllerInput;
use crate::polytools::{CharacterAssetLibrary, spawn_character_visual};

const VIEWPORT_WIDTH_METERS: f32 = 15.0;
const VIEWPORT_HEIGHT_METERS: f32 = 9.375;
const SELECTION_HEIGHT_METERS: f32 = VIEWPORT_HEIGHT_METERS * 0.75;
const PANEL_WIDTH_METERS: f32 = VIEWPORT_WIDTH_METERS / 5.0;
const PREVIEW_SCALE: f32 = 0.95;
const CORRECTION_HALF_LIFE_SECONDS: f32 = 0.2;
const CORRECTION_EPSILON_SQUARED: f32 = 0.000_001;
const CONTROLLER_STICK_DEADZONE: f32 = 0.15;
const STANDARD_ROOM_WIDTH_TILES: u32 = 15;
const STANDARD_ROOM_HEIGHT_TILES: u32 = 9;
const STANDARD_ROOM_CENTER_Y: f32 = 0.1875;
const PRIMARY_CHECKERBOARD_EVEN_COLOR: Color = Color::srgb(0.37, 0.35, 0.40);
const PRIMARY_CHECKERBOARD_ODD_COLOR: Color = Color::srgb(0.31, 0.29, 0.34);
const ALTERNATE_CHECKERBOARD_EVEN_COLOR: Color = Color::srgb(0.36, 0.39, 0.43);
const ALTERNATE_CHECKERBOARD_ODD_COLOR: Color = Color::srgb(0.30, 0.33, 0.37);

#[derive(Resource, Debug, Clone, Copy, Default, Eq, PartialEq)]
enum PresentationZoom {
    #[default]
    Standard,
    Expanded,
}

impl PresentationZoom {
    const fn viewport_width(self) -> f32 {
        match self {
            Self::Standard => 15.0,
            Self::Expanded => 20.0,
        }
    }

    const fn viewport_height(self) -> f32 {
        match self {
            Self::Standard => 9.375,
            Self::Expanded => 12.5,
        }
    }

    const fn bottom_ui_bar_height(self) -> f32 {
        match self {
            Self::Standard => 0.375,
            Self::Expanded => 0.5,
        }
    }

    const fn extra_tiles_left(self) -> f32 {
        match self {
            Self::Standard => 0.0,
            Self::Expanded => 2.0,
        }
    }

    const fn extra_tiles_below(self) -> f32 {
        match self {
            Self::Standard => 0.0,
            Self::Expanded => 1.0,
        }
    }

    const fn toggled(self) -> Self {
        match self {
            Self::Standard => Self::Expanded,
            Self::Expanded => Self::Standard,
        }
    }

    fn camera_anchor(self, room_floor_minimum: Vec2) -> Vec2 {
        Vec2::new(
            room_floor_minimum.x - self.extra_tiles_left() + self.viewport_width() * 0.5,
            room_floor_minimum.y - self.extra_tiles_below() + self.viewport_height() * 0.5
                - self.bottom_ui_bar_height(),
        )
    }
}

pub struct ClientPresentationPlugin {
    pub client_id: u64,
    pub tick_duration: std::time::Duration,
    pub snapshot_interval: std::time::Duration,
    pub remote_interpolation_ratio: f32,
    pub network_simulation: NetworkSimulationProfile,
    pub character_assets: CharacterAssetLibrary,
}

impl Plugin for ClientPresentationPlugin {
    fn build(&self, app: &mut App) {
        configure_client(app, self.tick_duration, self.snapshot_interval);
        app.insert_resource(Time::<Fixed>::from_duration(self.tick_duration))
            .init_state::<ClientScreen>()
            .init_resource::<PresentationZoom>()
            .insert_resource(ClientSession {
                client_id: self.client_id,
                remote_interpolation_ratio: self.remote_interpolation_ratio,
                network_simulation: self.network_simulation,
                selected: None,
                joining: false,
            })
            .insert_resource(self.character_assets.clone())
            .add_systems(Startup, setup_selection)
            .add_systems(OnEnter(ClientScreen::InGame), spawn_bottom_ui_bar)
            .add_systems(OnExit(ClientScreen::CharacterSelection), cleanup_selection)
            .add_systems(
                Update,
                (
                    apply_letterbox_viewport,
                    (handle_selection_input, update_selection_feedback)
                        .chain()
                        .run_if(in_state(ClientScreen::CharacterSelection)),
                    collect_movement_input,
                    toggle_presentation_zoom.run_if(in_state(ClientScreen::InGame)),
                    (render_new_players, initialize_local_render_history)
                        .chain()
                        .run_if(in_state(ClientScreen::InGame)),
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
                    center_camera_on_local_room,
                    sync_bottom_ui_bar_position,
                )
                    .chain()
                    .run_if(in_state(ClientScreen::InGame)),
            );
    }
}

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
enum ClientScreen {
    #[default]
    CharacterSelection,
    InGame,
}

fn collect_movement_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controller_input: NonSendMut<ControllerInput>,
    mut input: ResMut<ClientMovementInput>,
) {
    let keyboard_direction = Vec2::new(
        axis(&keyboard, KeyCode::KeyD, KeyCode::KeyA),
        axis(&keyboard, KeyCode::KeyW, KeyCode::KeyS),
    );
    let direction = movement_direction(keyboard_direction, controller_input.left_stick());
    input.0 = MovementIntent::new(direction.x, direction.y);
}

fn toggle_presentation_zoom(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut zoom: ResMut<PresentationZoom>,
    mut cameras: Query<&mut Projection, With<PresentationCamera>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyZ) {
        return;
    }

    *zoom = zoom.toggled();
    for mut projection in &mut cameras {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scaling_mode = ScalingMode::FixedVertical {
                viewport_height: zoom.viewport_height(),
            };
        }
    }
}

fn axis(keyboard: &ButtonInput<KeyCode>, positive: KeyCode, negative: KeyCode) -> f32 {
    f32::from(keyboard.pressed(positive)) - f32::from(keyboard.pressed(negative))
}

fn movement_direction(keyboard_direction: Vec2, controller_direction: Option<Vec2>) -> Vec2 {
    controller_direction
        .and_then(controller_stick_direction)
        .unwrap_or_else(|| keyboard_direction.normalize_or_zero())
}

fn controller_stick_direction(stick: Vec2) -> Option<Vec2> {
    if !stick.is_finite() {
        return None;
    }

    let magnitude = stick.length();
    if magnitude <= CONTROLLER_STICK_DEADZONE {
        return None;
    }

    let scaled_magnitude =
        ((magnitude - CONTROLLER_STICK_DEADZONE) / (1.0 - CONTROLLER_STICK_DEADZONE)).min(1.0);
    Some(stick.normalize_or_zero() * scaled_magnitude)
}

#[derive(Resource)]
struct ClientSession {
    client_id: u64,
    remote_interpolation_ratio: f32,
    network_simulation: NetworkSimulationProfile,
    selected: Option<CharacterKind>,
    joining: bool,
}

#[derive(Component)]
struct SelectionVisual;

#[derive(Component)]
struct SelectionPreview(CharacterKind);

#[derive(Component)]
struct SelectionButton(CharacterKind);

#[derive(Component)]
struct ConfirmButton;

#[derive(Component)]
struct ConfirmButtonLabel;

#[derive(Component)]
struct RenderedCharacter;

#[derive(Component)]
struct PresentationCamera;

#[derive(Component)]
struct BottomUiBar;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct LocalRenderHistory {
    previous: Vec2,
    current: Vec2,
}

fn setup_selection(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    character_assets: Res<CharacterAssetLibrary>,
) {
    commands.spawn((
        Camera2d,
        PresentationCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: PresentationZoom::Standard.viewport_height(),
            },
            ..OrthographicProjection::default_2d()
        }),
    ));

    let selection_center_y = (VIEWPORT_HEIGHT_METERS - SELECTION_HEIGHT_METERS) * 0.5;
    for (index, character) in CharacterKind::ALL.into_iter().enumerate() {
        let x = -VIEWPORT_WIDTH_METERS * 0.5 + PANEL_WIDTH_METERS * (index as f32 + 0.5);
        let root = commands
            .spawn((
                Transform::from_xyz(x, selection_center_y + 0.15, 0.0)
                    .with_scale(Vec3::splat(PREVIEW_SCALE)),
                Visibility::default(),
                SelectionVisual,
                SelectionPreview(character),
            ))
            .id();
        if let Err(error) = spawn_character_visual(
            &mut commands,
            root,
            &mut meshes,
            &mut materials,
            &character_assets,
            character,
        ) {
            error!("cannot spawn PolyTools selection visual: {error}");
        }
    }

    commands.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        SelectionVisual,
        children![
            (
                Node {
                    width: percent(100),
                    height: percent(75),
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                SelectionVisual,
                children![
                    (selection_button(CharacterKind::Wizard), SelectionVisual),
                    (selection_button(CharacterKind::Mage), SelectionVisual),
                    (selection_button(CharacterKind::Sorcerer), SelectionVisual),
                    (selection_button(CharacterKind::Rogue), SelectionVisual),
                    (selection_button(CharacterKind::Glavier), SelectionVisual),
                ]
            ),
            (
                Node {
                    width: percent(100),
                    height: percent(25),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                SelectionVisual,
                children![confirm_button()]
            )
        ],
    ));
}

fn apply_letterbox_viewport(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<PresentationCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    camera.viewport = Some(letterbox_viewport(window.physical_size()));
}

fn letterbox_viewport(window_size: UVec2) -> Viewport {
    if window_size.x == 0 || window_size.y == 0 {
        return Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::ZERO,
            depth: 0.0..1.0,
        };
    }

    let width_from_height = u64::from(window_size.y) * 16 / 10;
    let viewport_size = if width_from_height <= u64::from(window_size.x) {
        UVec2::new(width_from_height as u32, window_size.y)
    } else {
        UVec2::new(window_size.x, (u64::from(window_size.x) * 10 / 16) as u32)
    };

    Viewport {
        physical_position: (window_size - viewport_size) / 2,
        physical_size: viewport_size,
        depth: 0.0..1.0,
    }
}

fn selection_button(character: CharacterKind) -> impl Bundle {
    (
        Button,
        SelectionButton(character),
        Node {
            width: percent(20),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexEnd,
            padding: UiRect::new(px(8), px(8), px(8), px(18)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.85, 0.85, 0.9, 0.35)),
        BackgroundColor(panel_color(character, false).with_alpha(0.30)),
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
) -> Result {
    if session.joining {
        return Ok(());
    }

    for (interaction, selection_button, confirm_button) in &buttons {
        if *interaction == Interaction::Pressed {
            if let Some(selection_button) = selection_button {
                session.selected = Some(selection_button.0);
            } else if confirm_button.is_some() {
                return join_selected_character(&mut session, &mut commands, &mut next_screen);
            }
        }
    }

    for (key, character) in [
        (KeyCode::Digit1, CharacterKind::Wizard),
        (KeyCode::Digit2, CharacterKind::Mage),
        (KeyCode::Digit3, CharacterKind::Sorcerer),
        (KeyCode::Digit4, CharacterKind::Rogue),
        (KeyCode::Digit5, CharacterKind::Glavier),
    ] {
        if keyboard.just_pressed(key) {
            session.selected = Some(character);
        }
    }

    if keyboard.just_pressed(KeyCode::ArrowLeft) {
        session.selected = Some(adjacent_character(session.selected, -1));
    } else if keyboard.just_pressed(KeyCode::ArrowRight) {
        session.selected = Some(adjacent_character(session.selected, 1));
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
            let selected = session.selected == Some(selection_button.0);
            if *interaction == Interaction::Hovered {
                hovered_character = Some(selection_button.0);
            }
            let intensity = match (*interaction, selected) {
                (Interaction::Pressed, _) => 0.72,
                (_, true) => 0.62,
                (Interaction::Hovered, false) => 0.48,
                _ => 0.30,
            };
            **background =
                panel_color(selection_button.0, selected || intensity > 0.30).with_alpha(intensity);
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
        let scale = if session.selected == Some(preview.0) {
            PREVIEW_SCALE * 1.08
        } else if hovered_character == Some(preview.0) {
            PREVIEW_SCALE * 1.04
        } else {
            PREVIEW_SCALE
        };
        transform.scale = Vec3::splat(scale);
    }
}

fn adjacent_character(selected: Option<CharacterKind>, offset: isize) -> CharacterKind {
    let current = selected
        .and_then(|selected| {
            CharacterKind::ALL
                .iter()
                .position(|candidate| *candidate == selected)
        })
        .unwrap_or(if offset < 0 {
            0
        } else {
            CharacterKind::ALL.len() - 1
        });
    let next = (current as isize + offset).rem_euclid(CharacterKind::ALL.len() as isize) as usize;
    CharacterKind::ALL[next]
}

fn join_selected_character(
    session: &mut ClientSession,
    commands: &mut Commands,
    next_screen: &mut NextState<ClientScreen>,
) -> Result {
    let Some(character) = session.selected else {
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
    spawn_starting_room_neighborhood(commands);
    next_screen.set(ClientScreen::InGame);
    Ok(())
}

fn cleanup_selection(
    selection_visuals: Query<Entity, With<SelectionVisual>>,
    mut commands: Commands,
) {
    for entity in &selection_visuals {
        commands.entity(entity).despawn();
    }
}

fn spawn_bottom_ui_bar(mut commands: Commands, zoom: Res<PresentationZoom>) {
    commands.spawn((
        BottomUiBar,
        Sprite::from_color(
            Color::BLACK,
            Vec2::new(zoom.viewport_width(), zoom.bottom_ui_bar_height()),
        ),
        Transform::from_translation(bottom_ui_bar_position(Vec2::ZERO, *zoom)),
    ));
}

fn spawn_starting_room_neighborhood(commands: &mut Commands) {
    for room_y in -1..=1 {
        for room_x in -1..=1 {
            spawn_standard_room(commands, IVec2::new(room_x, room_y));
        }
    }
}

fn spawn_standard_room(commands: &mut Commands, room_coordinates: IVec2) {
    for row in 0..STANDARD_ROOM_HEIGHT_TILES {
        for column in 0..STANDARD_ROOM_WIDTH_TILES {
            let color = checkerboard_color(room_coordinates, row, column);
            let x =
                room_coordinates.x as f32 * STANDARD_ROOM_WIDTH_TILES as f32 + column as f32 + 0.5
                    - STANDARD_ROOM_WIDTH_TILES as f32 * 0.5;
            let y = STANDARD_ROOM_CENTER_Y
                + room_coordinates.y as f32 * STANDARD_ROOM_HEIGHT_TILES as f32
                + row as f32
                + 0.5
                - STANDARD_ROOM_HEIGHT_TILES as f32 * 0.5;
            commands.spawn((
                Sprite::from_color(color, Vec2::ONE),
                Transform::from_xyz(x, y, -10.0),
            ));
        }
    }
}

fn checkerboard_color(room_coordinates: IVec2, row: u32, column: u32) -> Color {
    let primary_room = (room_coordinates.x + room_coordinates.y).rem_euclid(2) == 0;
    let even_tile = (row + column) % 2 == 0;

    match (primary_room, even_tile) {
        (true, true) => PRIMARY_CHECKERBOARD_EVEN_COLOR,
        (true, false) => PRIMARY_CHECKERBOARD_ODD_COLOR,
        (false, true) => ALTERNATE_CHECKERBOARD_EVEN_COLOR,
        (false, false) => ALTERNATE_CHECKERBOARD_ODD_COLOR,
    }
}

fn render_new_players(
    mut commands: Commands,
    players: Query<(Entity, &SelectedCharacter, &Position), Without<RenderedCharacter>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    character_assets: Res<CharacterAssetLibrary>,
) {
    for (entity, character, position) in &players {
        commands.entity(entity).insert((
            RenderedCharacter,
            Visibility::default(),
            Transform::from_xyz(position.x, position.y, 0.0),
        ));
        if let Err(error) = spawn_character_visual(
            &mut commands,
            entity,
            &mut meshes,
            &mut materials,
            &character_assets,
            character.0,
        ) {
            error!("cannot spawn PolyTools player visual: {error}");
        }
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
            &mut Transform,
        ),
        With<RenderedCharacter>,
    >,
    mut commands: Commands,
) {
    let alpha = fixed_time.overstep_fraction();
    let correction_decay = correction_decay(virtual_time.delta_secs());

    for (entity, position, history, extrapolation, correction, mut transform) in &mut players {
        let mut rendered = sampled_render_position(*position, history, alpha);
        if history.is_none()
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

fn center_camera_on_local_room(
    room_grid: Res<StartingRoomGrid>,
    zoom: Res<PresentationZoom>,
    local_players: Query<&RoomId, (With<RenderedCharacter>, With<MovementIntent>)>,
    mut cameras: Query<&mut Transform, With<PresentationCamera>>,
) {
    let Ok(room) = local_players.single() else {
        return;
    };
    let Ok(mut camera_transform) = cameras.single_mut() else {
        return;
    };

    let anchor = zoom.camera_anchor(room_grid.room_floor_minimum(*room));
    camera_transform.translation.x = anchor.x;
    camera_transform.translation.y = anchor.y;
}

fn sync_bottom_ui_bar_position(
    zoom: Res<PresentationZoom>,
    cameras: Query<&Transform, (With<PresentationCamera>, Without<BottomUiBar>)>,
    mut ui_bars: Query<
        (&mut Transform, &mut Sprite),
        (With<BottomUiBar>, Without<PresentationCamera>),
    >,
) {
    let Ok(camera_transform) = cameras.single() else {
        return;
    };

    for (mut ui_bar_transform, mut sprite) in &mut ui_bars {
        ui_bar_transform.translation =
            bottom_ui_bar_position(camera_transform.translation.truncate(), *zoom);
        sprite.custom_size = Some(Vec2::new(
            zoom.viewport_width(),
            zoom.bottom_ui_bar_height(),
        ));
    }
}

fn bottom_ui_bar_position(camera_position: Vec2, zoom: PresentationZoom) -> Vec3 {
    Vec3::new(
        camera_position.x,
        camera_position.y - zoom.viewport_height() * 0.5 + zoom.bottom_ui_bar_height() * 0.5,
        100.0,
    )
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

fn panel_color(character: CharacterKind, selected: bool) -> Color {
    let base = match character {
        CharacterKind::Wizard => (0.16, 0.12, 0.28),
        CharacterKind::Mage => (0.10, 0.24, 0.27),
        CharacterKind::Sorcerer => (0.29, 0.10, 0.12),
        CharacterKind::Rogue => (0.12, 0.13, 0.15),
        CharacterKind::Glavier => (0.27, 0.20, 0.08),
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
        assert_eq!(
            adjacent_character(Some(CharacterKind::Wizard), -1),
            CharacterKind::Glavier
        );
        assert_eq!(
            adjacent_character(Some(CharacterKind::Glavier), 1),
            CharacterKind::Wizard
        );
    }

    #[test]
    fn keyboard_selection_starts_at_directional_edge() {
        assert_eq!(adjacent_character(None, 1), CharacterKind::Wizard);
        assert_eq!(adjacent_character(None, -1), CharacterKind::Glavier);
    }

    #[test]
    fn controller_direction_overrides_keyboard_direction() {
        assert_eq!(movement_direction(Vec2::X, Some(Vec2::Y)), Vec2::Y);
    }

    #[test]
    fn neutral_controller_direction_uses_keyboard_fallback() {
        assert_eq!(
            movement_direction(Vec2::new(1.0, 1.0), Some(Vec2::ZERO)),
            Vec2::new(1.0, 1.0).normalize(),
        );
    }

    #[test]
    fn controller_deadzone_blocks_small_stick_drift() {
        assert_eq!(controller_stick_direction(Vec2::new(0.15, 0.0)), None);
    }

    #[test]
    fn controller_stick_preserves_partial_movement_strength() {
        let direction = controller_stick_direction(Vec2::new(0.575, 0.0))
            .expect("stick outside the deadzone produces movement");
        assert!((direction.x - 0.5).abs() < f32::EPSILON);
        assert_eq!(direction.y, 0.0);
    }

    #[test]
    fn standard_room_checkerboard_matches_tile_dimensions() {
        assert_eq!(STANDARD_ROOM_WIDTH_TILES * STANDARD_ROOM_HEIGHT_TILES, 135);
        assert_eq!(
            STANDARD_ROOM_CENTER_Y - STANDARD_ROOM_HEIGHT_TILES as f32 * 0.5,
            -4.3125
        );
    }

    #[test]
    fn room_checkerboard_palette_alternates_by_cardinal_neighbor() {
        let center = checkerboard_color(IVec2::ZERO, 0, 0);
        assert_eq!(center, checkerboard_color(IVec2::new(1, 1), 0, 0));
        assert_eq!(center, checkerboard_color(IVec2::new(-1, 1), 0, 0));
        assert_ne!(center, checkerboard_color(IVec2::new(1, 0), 0, 0));
        assert_ne!(center, checkerboard_color(IVec2::new(0, -1), 0, 0));
    }

    #[test]
    fn starting_neighborhood_contains_nine_standard_rooms() {
        assert_eq!(
            3 * 3 * STANDARD_ROOM_WIDTH_TILES * STANDARD_ROOM_HEIGHT_TILES,
            1215
        );
    }

    #[test]
    fn letterbox_keeps_the_full_16_by_10_viewport_inside_widescreen() {
        let viewport = letterbox_viewport(UVec2::new(1920, 1080));
        assert_eq!(viewport.physical_position, UVec2::new(96, 0));
        assert_eq!(viewport.physical_size, UVec2::new(1728, 1080));
    }

    #[test]
    fn letterbox_keeps_the_full_16_by_10_viewport_inside_tall_windows() {
        let viewport = letterbox_viewport(UVec2::new(1200, 1200));
        assert_eq!(viewport.physical_position, UVec2::new(0, 225));
        assert_eq!(viewport.physical_size, UVec2::new(1200, 750));
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
    fn camera_keeps_the_full_free_strip_below_the_local_players_current_room() {
        let mut app = App::new();
        app.init_resource::<StartingRoomGrid>()
            .init_resource::<PresentationZoom>()
            .add_systems(Update, center_camera_on_local_room);
        let camera = app
            .world_mut()
            .spawn((PresentationCamera, Transform::default()))
            .id();
        app.world_mut()
            .spawn((RenderedCharacter, MovementIntent::ZERO, RoomId(6)));

        app.update();

        assert_eq!(
            app.world()
                .get::<Transform>(camera)
                .map(|transform| transform.translation),
            Some(Vec3::new(-15.0, 9.0, 0.0))
        );
    }

    #[test]
    fn bottom_ui_bar_exactly_covers_the_free_strip_below_the_room_floor() {
        assert_eq!(
            bottom_ui_bar_position(Vec2::new(15.0, -9.0), PresentationZoom::Standard),
            Vec3::new(15.0, -13.5, 100.0)
        );
    }

    #[test]
    fn expanded_zoom_shows_twenty_by_twelve_complete_floor_tiles() {
        assert_eq!(PresentationZoom::Expanded.viewport_width(), 20.0);
        assert_eq!(
            PresentationZoom::Expanded.viewport_height()
                - PresentationZoom::Expanded.bottom_ui_bar_height(),
            12.0
        );
        assert_eq!(
            PresentationZoom::Expanded.camera_anchor(Vec2::new(-7.5, -4.3125)),
            Vec2::new(0.5, 0.4375)
        );
        assert_eq!(
            bottom_ui_bar_position(Vec2::new(0.5, 0.4375), PresentationZoom::Expanded),
            Vec3::new(0.5, -5.5625, 100.0)
        );
    }

    #[test]
    fn z_toggles_the_camera_projection_to_the_expanded_zoom() {
        let mut app = App::new();
        app.init_resource::<PresentationZoom>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .add_systems(Update, toggle_presentation_zoom);
        let camera = app
            .world_mut()
            .spawn((
                PresentationCamera,
                Projection::Orthographic(OrthographicProjection::default_2d()),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyZ);

        app.update();

        assert_eq!(
            app.world().resource::<PresentationZoom>(),
            &PresentationZoom::Expanded
        );
        let Some(Projection::Orthographic(projection)) = app.world().get::<Projection>(camera)
        else {
            panic!("presentation camera retains an orthographic projection");
        };
        let ScalingMode::FixedVertical { viewport_height } = projection.scaling_mode else {
            panic!("zoom toggle uses a fixed vertical camera projection");
        };
        assert_eq!(viewport_height, 12.5);
    }

    #[test]
    fn bottom_ui_bar_tracks_the_camera_without_transform_query_conflicts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PresentationZoom>()
            .add_systems(Update, sync_bottom_ui_bar_position);
        app.world_mut()
            .spawn((PresentationCamera, Transform::from_xyz(-15.0, 9.0, 0.0)));
        let bar = app
            .world_mut()
            .spawn((BottomUiBar, Sprite::default(), Transform::default()))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<Transform>(bar)
                .map(|transform| transform.translation),
            Some(Vec3::new(-15.0, 4.5, 100.0))
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

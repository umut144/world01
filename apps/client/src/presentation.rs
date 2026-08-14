use bevy::{camera::ScalingMode, prelude::*};
use game01_network::{ClientMovementInput, configure_client, connect_client};
use game01_world_data::{CharacterKind, MovementIntent, Position, SelectedCharacter};

const VIEWPORT_WIDTH_METERS: f32 = 15.0;
const VIEWPORT_HEIGHT_METERS: f32 = 9.375;
const SELECTION_HEIGHT_METERS: f32 = VIEWPORT_HEIGHT_METERS * 0.75;
const PANEL_WIDTH_METERS: f32 = VIEWPORT_WIDTH_METERS / 5.0;
const PREVIEW_SCALE: f32 = 0.95;

pub struct ClientPresentationPlugin {
    pub client_id: u64,
    pub tick_duration: std::time::Duration,
}

impl Plugin for ClientPresentationPlugin {
    fn build(&self, app: &mut App) {
        configure_client(app, self.tick_duration);
        app.insert_resource(Time::<Fixed>::from_duration(self.tick_duration))
            .init_state::<ClientScreen>()
            .insert_resource(ClientSession {
                client_id: self.client_id,
                selected: None,
                joining: false,
            })
            .add_systems(Startup, setup_selection)
            .add_systems(OnExit(ClientScreen::CharacterSelection), cleanup_selection)
            .add_systems(
                Update,
                (
                    (handle_selection_input, update_selection_feedback)
                        .chain()
                        .run_if(in_state(ClientScreen::CharacterSelection)),
                    collect_movement_input,
                    render_new_players.run_if(in_state(ClientScreen::InGame)),
                    sync_rendered_positions.run_if(in_state(ClientScreen::InGame)),
                ),
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
    mut input: ResMut<ClientMovementInput>,
) {
    let x = axis(&keyboard, KeyCode::KeyD, KeyCode::KeyA);
    let y = axis(&keyboard, KeyCode::KeyW, KeyCode::KeyS);
    let direction = Vec2::new(x, y).normalize_or_zero();
    input.0 = MovementIntent::new(direction.x, direction.y);
}

fn axis(keyboard: &ButtonInput<KeyCode>, positive: KeyCode, negative: KeyCode) -> f32 {
    f32::from(keyboard.pressed(positive)) - f32::from(keyboard.pressed(negative))
}

#[derive(Resource)]
struct ClientSession {
    client_id: u64,
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

fn setup_selection(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEWPORT_HEIGHT_METERS,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));

    let selection_center_y = (VIEWPORT_HEIGHT_METERS - SELECTION_HEIGHT_METERS) * 0.5;
    for (index, character) in CharacterKind::ALL.into_iter().enumerate() {
        let x = -VIEWPORT_WIDTH_METERS * 0.5 + PANEL_WIDTH_METERS * (index as f32 + 0.5);
        let mut root = commands.spawn((
            Transform::from_xyz(x, selection_center_y + 0.15, 0.0)
                .with_scale(Vec3::splat(PREVIEW_SCALE)),
            Visibility::default(),
            SelectionVisual,
            SelectionPreview(character),
        ));
        attach_character_visual(&mut root, &mut meshes, &mut materials, character);
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

    connect_client(commands, session.client_id, character)?;
    session.joining = true;
    spawn_standard_room(commands);
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

fn spawn_standard_room(commands: &mut Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgb(0.105, 0.095, 0.115), Vec2::new(15.0, 9.0)),
        Transform::from_xyz(0.0, 0.1875, -10.0),
    ));
    commands.spawn((
        Sprite::from_color(Color::srgb(0.24, 0.21, 0.27), Vec2::new(14.7, 0.10)),
        Transform::from_xyz(0.0, 4.62, -9.0),
    ));
    commands.spawn((
        Sprite::from_color(Color::srgb(0.24, 0.21, 0.27), Vec2::new(14.7, 0.10)),
        Transform::from_xyz(0.0, -4.245, -9.0),
    ));
}

fn render_new_players(
    mut commands: Commands,
    players: Query<(Entity, &SelectedCharacter, &Position), Without<RenderedCharacter>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, character, position) in &players {
        let mut player = commands.entity(entity);
        player.insert((
            RenderedCharacter,
            Visibility::default(),
            Transform::from_xyz(position.x, position.y, 0.0),
        ));
        attach_character_visual(&mut player, &mut meshes, &mut materials, character.0);
    }
}

fn sync_rendered_positions(
    mut players: Query<(&Position, &mut Transform), With<RenderedCharacter>>,
) {
    for (position, mut transform) in &mut players {
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}

fn attach_character_visual(
    root: &mut EntityCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    character: CharacterKind,
) {
    let head_mesh = meshes.add(Circle::new(0.38));
    let hat_mesh = meshes.add(Triangle2d::new(
        Vec2::new(0.0, 1.55),
        Vec2::new(-0.82, 0.35),
        Vec2::new(0.82, 0.35),
    ));
    let blade_mesh = meshes.add(Triangle2d::new(
        Vec2::new(0.0, 0.28),
        Vec2::new(-0.12, -0.18),
        Vec2::new(0.12, -0.18),
    ));
    let skin = materials.add(Color::srgb(0.82, 0.63, 0.48));
    let primary = materials.add(character_color(character));
    let metal = materials.add(Color::srgb(0.72, 0.78, 0.82));
    let (hat_transform, brim_size) = headwear_geometry(character);

    root.with_children(|parent| {
        parent.spawn((
            Sprite::from_color(character_color(character), Vec2::new(1.15, 1.45)),
            Transform::from_xyz(0.0, -0.55, 0.0),
        ));
        parent.spawn((
            Mesh2d(head_mesh),
            MeshMaterial2d(skin),
            Transform::from_xyz(0.0, 0.38, 1.0),
        ));
        parent.spawn((Mesh2d(hat_mesh), MeshMaterial2d(primary), hat_transform));
        parent.spawn((
            Sprite::from_color(Color::srgb(0.04, 0.04, 0.055), brim_size),
            Transform::from_xyz(0.0, 0.58, 3.0),
        ));
        for x in [-0.15, 0.15] {
            parent.spawn((
                Sprite::from_color(Color::srgb(0.03, 0.03, 0.04), Vec2::splat(0.09)),
                Transform::from_xyz(x, 0.40, 4.0),
            ));
        }
        parent.spawn((
            Sprite::from_color(Color::srgb(0.22, 0.08, 0.07), Vec2::new(0.22, 0.055)),
            Transform::from_xyz(0.0, 0.17, 4.0),
        ));

        match character {
            CharacterKind::Wizard => {
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.91, 0.77, 0.20), Vec2::splat(0.16)),
                    Transform::from_xyz(-0.52, 1.10, 4.0)
                        .with_rotation(Quat::from_rotation_z(0.22)),
                ));
            }
            CharacterKind::Mage => {
                for x in [-0.34, 0.34] {
                    parent.spawn((
                        Sprite::from_color(Color::srgb(0.30, 0.16, 0.10), Vec2::new(0.16, 0.78)),
                        Transform::from_xyz(x, 0.08, 0.5),
                    ));
                }
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.18, 0.72, 0.78), Vec2::new(0.55, 0.18)),
                    Transform::from_xyz(0.48, 1.18, 3.0)
                        .with_rotation(Quat::from_rotation_z(-0.65)),
                ));
            }
            CharacterKind::Sorcerer => {
                parent.spawn((
                    Sprite::from_color(Color::WHITE, Vec2::new(0.14, 0.55)),
                    Transform::from_xyz(0.0, 1.0, 4.0),
                ));
                parent.spawn((
                    Sprite::from_color(Color::WHITE, Vec2::new(0.48, 0.14)),
                    Transform::from_xyz(0.0, 1.0, 4.0),
                ));
            }
            CharacterKind::Rogue => {
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.07, 0.075, 0.09), Vec2::new(0.62, 0.16)),
                    Transform::from_xyz(-0.45, 0.72, 4.0)
                        .with_rotation(Quat::from_rotation_z(-0.48)),
                ));
                for (x, rotation) in [(-0.72, -0.55), (0.72, 0.55)] {
                    parent.spawn((
                        Sprite::from_color(Color::srgb(0.28, 0.22, 0.18), Vec2::new(0.10, 0.75)),
                        Transform::from_xyz(x, -0.30, 3.0)
                            .with_rotation(Quat::from_rotation_z(rotation)),
                    ));
                    parent.spawn((
                        Mesh2d(blade_mesh.clone()),
                        MeshMaterial2d(metal.clone()),
                        Transform::from_xyz(x * 1.18, 0.05, 3.0)
                            .with_rotation(Quat::from_rotation_z(rotation)),
                    ));
                }
            }
            CharacterKind::Glavier => {
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.73, 0.52, 0.16), Vec2::new(0.58, 0.20)),
                    Transform::from_xyz(0.0, 0.72, 4.0),
                ));
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.32, 0.22, 0.12), Vec2::new(0.10, 2.55)),
                    Transform::from_xyz(0.72, 0.08, 3.0)
                        .with_rotation(Quat::from_rotation_z(-0.18)),
                ));
                parent.spawn((
                    Mesh2d(blade_mesh),
                    MeshMaterial2d(metal),
                    Transform::from_xyz(0.94, 1.36, 3.0)
                        .with_scale(Vec3::new(1.45, 1.45, 1.0))
                        .with_rotation(Quat::from_rotation_z(-0.18)),
                ));
            }
        }
    });
}

fn headwear_geometry(character: CharacterKind) -> (Transform, Vec2) {
    match character {
        CharacterKind::Wizard => (
            Transform::from_xyz(-0.08, 0.24, 2.0)
                .with_scale(Vec3::new(1.05, 0.96, 1.0))
                .with_rotation(Quat::from_rotation_z(0.10)),
            Vec2::new(1.82, 0.15),
        ),
        CharacterKind::Mage => (
            Transform::from_xyz(0.10, 0.22, 2.0)
                .with_scale(Vec3::new(0.96, 1.08, 1.0))
                .with_rotation(Quat::from_rotation_z(-0.12)),
            Vec2::new(1.65, 0.13),
        ),
        CharacterKind::Sorcerer => (
            Transform::from_xyz(0.0, 0.20, 2.0).with_scale(Vec3::new(0.86, 1.18, 1.0)),
            Vec2::new(1.52, 0.14),
        ),
        CharacterKind::Rogue => (
            Transform::from_xyz(-0.02, 0.30, 2.0)
                .with_scale(Vec3::new(1.06, 0.72, 1.0))
                .with_rotation(Quat::from_rotation_z(0.05)),
            Vec2::new(1.72, 0.13),
        ),
        CharacterKind::Glavier => (
            Transform::from_xyz(0.0, 0.28, 2.0).with_scale(Vec3::new(0.82, 0.88, 1.0)),
            Vec2::new(1.46, 0.14),
        ),
    }
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

fn character_color(character: CharacterKind) -> Color {
    match character {
        CharacterKind::Wizard => Color::srgb(0.31, 0.18, 0.58),
        CharacterKind::Mage => Color::srgb(0.12, 0.55, 0.62),
        CharacterKind::Sorcerer => Color::srgb(0.62, 0.12, 0.16),
        CharacterKind::Rogue => Color::srgb(0.16, 0.17, 0.21),
        CharacterKind::Glavier => Color::srgb(0.63, 0.43, 0.11),
    }
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
    fn presentation_sync_only_derives_translation_from_position() {
        let mut app = App::new();
        app.add_systems(Update, sync_rendered_positions);
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
}

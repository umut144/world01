use bevy::{camera::ScalingMode, prelude::*, window::PrimaryWindow};
use game01_network::{configure_client, connect_client};
use game01_world_data::{CharacterKind, SelectedCharacter};

const VIEWPORT_WIDTH_METERS: f32 = 15.0;
const VIEWPORT_HEIGHT_METERS: f32 = 9.375;
const SELECTION_HEIGHT_METERS: f32 = VIEWPORT_HEIGHT_METERS * 0.75;
const PANEL_WIDTH_METERS: f32 = VIEWPORT_WIDTH_METERS / 5.0;

pub struct ClientPresentationPlugin {
    pub client_id: u64,
    pub tick_duration: std::time::Duration,
}

impl Plugin for ClientPresentationPlugin {
    fn build(&self, app: &mut App) {
        configure_client(app, self.tick_duration);
        app.insert_resource(ClientSession {
            client_id: self.client_id,
            selected: None,
            joining: false,
        })
        .add_systems(Startup, setup_selection)
        .add_systems(Update, (handle_selection_click, render_new_players));
    }
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
struct SelectionPanel(CharacterKind);

#[derive(Component)]
struct ConfirmButton;

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
        commands.spawn((
            Sprite::from_color(
                panel_color(character, false),
                Vec2::new(PANEL_WIDTH_METERS - 0.04, SELECTION_HEIGHT_METERS),
            ),
            Transform::from_xyz(x, selection_center_y, -5.0),
            SelectionPanel(character),
            SelectionVisual,
        ));

        let mut root = commands.spawn((
            Transform::from_xyz(x, selection_center_y + 0.25, 0.0).with_scale(Vec3::splat(1.35)),
            Visibility::default(),
            SelectionVisual,
        ));
        attach_character_visual(&mut root, &mut meshes, &mut materials, character);

        commands.spawn((
            Text2d::new(character.label()),
            TextFont::from_font_size(30.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Transform::from_xyz(x, -1.85, 2.0),
            SelectionVisual,
        ));
    }

    commands.spawn((
        Sprite::from_color(Color::srgb(0.18, 0.19, 0.22), Vec2::new(7.0, 1.0)),
        Transform::from_xyz(0.0, -3.55, 0.0),
        ConfirmButton,
        SelectionVisual,
    ));
    commands.spawn((
        Text2d::new("SELECT CHARACTER & JOIN"),
        TextFont::from_font_size(34.0),
        TextColor(Color::srgb(0.55, 0.55, 0.58)),
        TextLayout::justify(Justify::Center),
        Transform::from_xyz(0.0, -3.55, 2.0),
        SelectionVisual,
    ));
}

fn handle_selection_click(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut session: ResMut<ClientSession>,
    mut panels: Query<(&SelectionPanel, &mut Sprite)>,
    mut button: Single<&mut Sprite, With<ConfirmButton>>,
    selection_visuals: Query<Entity, With<SelectionVisual>>,
    mut commands: Commands,
) -> Result {
    if session.joining || !mouse.just_pressed(MouseButton::Left) {
        return Ok(());
    }
    let Some(cursor) = window.cursor_position() else {
        return Ok(());
    };
    let normalized = Vec2::new(cursor.x / window.width(), cursor.y / window.height());

    if normalized.y <= 0.75 {
        let index = (normalized.x * 5.0).floor() as usize;
        if let Some(character) = CharacterKind::ALL.get(index).copied() {
            session.selected = Some(character);
        }
    } else if (0.80..=0.95).contains(&normalized.y)
        && (0.25..=0.75).contains(&normalized.x)
        && let Some(character) = session.selected
    {
        connect_client(&mut commands, session.client_id, character)?;
        session.joining = true;
        for entity in &selection_visuals {
            commands.entity(entity).despawn();
        }
        spawn_standard_room(&mut commands);
        return Ok(());
    }

    for (panel, mut sprite) in &mut panels {
        sprite.color = panel_color(panel.0, session.selected == Some(panel.0));
    }
    button.color = if session.selected.is_some() {
        Color::srgb(0.38, 0.25, 0.12)
    } else {
        Color::srgb(0.18, 0.19, 0.22)
    };
    Ok(())
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
    players: Query<(Entity, &SelectedCharacter), Without<RenderedCharacter>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, character) in &players {
        let mut player = commands.entity(entity);
        player.insert((RenderedCharacter, Visibility::default()));
        attach_character_visual(&mut player, &mut meshes, &mut materials, character.0);
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
        parent.spawn((
            Mesh2d(hat_mesh),
            MeshMaterial2d(primary),
            Transform::from_xyz(0.0, 0.25, 2.0),
        ));
        parent.spawn((
            Sprite::from_color(Color::srgb(0.04, 0.04, 0.055), Vec2::new(1.72, 0.15)),
            Transform::from_xyz(0.0, 0.58, 3.0),
        ));
        for x in [-0.15, 0.15] {
            parent.spawn((
                Sprite::from_color(Color::srgb(0.03, 0.03, 0.04), Vec2::splat(0.09)),
                Transform::from_xyz(x, 0.40, 4.0),
            ));
        }

        match character {
            CharacterKind::Wizard => {
                parent.spawn((
                    Sprite::from_color(Color::srgb(0.91, 0.77, 0.20), Vec2::splat(0.16)),
                    Transform::from_xyz(-0.42, 1.02, 4.0),
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
                    Sprite::from_color(Color::srgb(0.32, 0.22, 0.12), Vec2::new(0.10, 2.9)),
                    Transform::from_xyz(0.82, 0.15, 3.0)
                        .with_rotation(Quat::from_rotation_z(-0.22)),
                ));
                parent.spawn((
                    Mesh2d(blade_mesh),
                    MeshMaterial2d(metal),
                    Transform::from_xyz(1.12, 1.58, 3.0)
                        .with_rotation(Quat::from_rotation_z(-0.22)),
                ));
            }
        }
    });
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

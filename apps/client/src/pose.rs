use bevy::prelude::*;
use game01_simulation::CharacterLifeRules;
use game01_world_data::{
    BodyFacing, CharacterLifeState, DeathConfirmationState, MovementDirection, StatusEffectState,
};

use game01_content::AuthoredFacing;

use crate::polytools::{CharacterVisual, CharacterVisualOrientation};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PoseSettings {
    pub neutral_head_offset_meters: f32,
    pub neutral_head_half_life_seconds: f32,
}

impl Default for PoseSettings {
    fn default() -> Self {
        Self {
            neutral_head_offset_meters: 0.06,
            neutral_head_half_life_seconds: 0.05,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterHead {
    owner: Entity,
    rest_translation: Vec3,
}

impl CharacterHead {
    pub fn new(owner: Entity, rest_translation: Vec3) -> Self {
        Self {
            owner,
            rest_translation,
        }
    }
}

pub fn apply_body_facing(
    players: Query<(&BodyFacing, &CharacterVisual)>,
    mut orientation_roots: Query<&mut Transform, With<CharacterVisualOrientation>>,
) {
    for (facing, visual) in &players {
        let Some(scale_x) = directional_pose_scale_x(visual.authored_facing, *facing) else {
            continue;
        };
        let Ok(mut transform) = orientation_roots.get_mut(visual.orientation_root) else {
            continue;
        };
        transform.scale.x = scale_x;
    }
}

pub fn apply_neutral_head_motion(
    time: Res<Time>,
    settings: Res<PoseSettings>,
    players: Query<(&MovementDirection, &CharacterVisual)>,
    mut heads: Query<(&CharacterHead, &mut Transform)>,
) {
    let blend =
        exponential_blend_factor(time.delta_secs(), settings.neutral_head_half_life_seconds);

    for (head, mut transform) in &mut heads {
        let Ok((direction, visual)) = players.get(head.owner) else {
            continue;
        };
        let Some(target) = neutral_head_target(
            head.rest_translation.truncate(),
            visual.authored_facing,
            *direction,
            settings.neutral_head_offset_meters,
        ) else {
            continue;
        };

        let current = transform.translation.truncate();
        let next = current.lerp(target, blend);
        let next = if next.distance_squared(target) <= f32::EPSILON {
            target
        } else {
            next
        };
        transform.translation.x = next.x;
        transform.translation.y = next.y;
    }
}

pub fn apply_character_status_presentation(
    rules: Res<CharacterLifeRules>,
    players: Query<(
        &CharacterVisual,
        Option<&StatusEffectState>,
        Option<&CharacterLifeState>,
        Option<&DeathConfirmationState>,
    )>,
    mut orientation_roots: Query<&mut Transform, With<CharacterVisualOrientation>>,
    mut outlines: Query<&mut Visibility>,
) {
    for (visual, status, life, confirmation) in &players {
        let life = life.copied().unwrap_or(CharacterLifeState::Alive);
        let status_active = status.is_some_and(|status| status.blocks_all_input());
        let confirmation_ticks = confirmation
            .map(|confirmation| confirmation.held_ticks as f32)
            .unwrap_or_default();
        let scale = match life {
            CharacterLifeState::Alive if status_active => 0.9,
            CharacterLifeState::Dead
            | CharacterLifeState::DeathConfirming
            | CharacterLifeState::Reviving => 0.9,
            _ => 1.0,
        };
        let rotation = match life {
            CharacterLifeState::Dead | CharacterLifeState::DeathConfirming => {
                -14.0_f32.to_radians() + rules.confirmation_angle_radians(confirmation_ticks)
            }
            CharacterLifeState::Reviving => -14.0_f32.to_radians(),
            CharacterLifeState::Alive if status_active => 14.0_f32.to_radians(),
            CharacterLifeState::Alive => 0.0,
        };
        if let Ok(mut transform) = orientation_roots.get_mut(visual.orientation_root) {
            let facing_sign = if transform.scale.x.is_sign_negative() {
                -1.0
            } else {
                1.0
            };
            let scale_vector = Vec2::new(facing_sign * scale, scale);
            let transformed_pivot =
                Vec2::from_angle(rotation).rotate(visual.body_pivot * scale_vector);
            let pivot_offset = visual.body_pivot - transformed_pivot;
            transform.translation.x = pivot_offset.x;
            transform.translation.y = pivot_offset.y;
            transform.scale = Vec3::new(scale_vector.x, scale_vector.y, 1.0);
            transform.rotation = Quat::from_rotation_z(rotation);
        }

        let hide_outline =
            !life.is_alive() || (matches!(life, CharacterLifeState::Alive) && status_active);
        for outline in &visual.outline_visuals {
            if let Ok(mut visibility) = outlines.get_mut(*outline) {
                *visibility = if hide_outline {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                };
            }
        }
    }
}

fn directional_pose_scale_x(authored_facing: AuthoredFacing, facing: BodyFacing) -> Option<f32> {
    match (authored_facing, facing) {
        (AuthoredFacing::Left | AuthoredFacing::Right, BodyFacing::Authored) => Some(1.0),
        (AuthoredFacing::Left, BodyFacing::Left) | (AuthoredFacing::Right, BodyFacing::Right) => {
            Some(1.0)
        }
        (AuthoredFacing::Left, BodyFacing::Right) | (AuthoredFacing::Right, BodyFacing::Left) => {
            Some(-1.0)
        }
        (AuthoredFacing::Neutral | AuthoredFacing::Top | AuthoredFacing::Down, _) => None,
    }
}

fn neutral_head_target(
    rest: Vec2,
    authored_facing: AuthoredFacing,
    direction: MovementDirection,
    offset_meters: f32,
) -> Option<Vec2> {
    if authored_facing != AuthoredFacing::Neutral {
        return None;
    }

    let direction = Vec2::new(direction.x, direction.y);
    let direction = if direction.is_finite() {
        direction.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    Some(rest + direction * offset_meters.max(0.0))
}

fn exponential_blend_factor(delta_seconds: f32, half_life_seconds: f32) -> f32 {
    if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
        return 0.0;
    }
    if !half_life_seconds.is_finite() || half_life_seconds <= 0.0 {
        return 1.0;
    }

    1.0 - 2.0_f32.powf(-delta_seconds / half_life_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 0.000_01;

    #[test]
    fn directional_pose_matches_replicated_and_authored_facing() {
        assert_eq!(
            directional_pose_scale_x(AuthoredFacing::Left, BodyFacing::Left),
            Some(1.0)
        );
        assert_eq!(
            directional_pose_scale_x(AuthoredFacing::Left, BodyFacing::Right),
            Some(-1.0)
        );
        assert_eq!(
            directional_pose_scale_x(AuthoredFacing::Right, BodyFacing::Right),
            Some(1.0)
        );
        assert_eq!(
            directional_pose_scale_x(AuthoredFacing::Right, BodyFacing::Left),
            Some(-1.0)
        );
        assert_eq!(
            directional_pose_scale_x(AuthoredFacing::Left, BodyFacing::Authored),
            Some(1.0)
        );
    }

    #[test]
    fn non_horizontal_authored_poses_do_not_flip() {
        for authored_facing in [
            AuthoredFacing::Neutral,
            AuthoredFacing::Top,
            AuthoredFacing::Down,
        ] {
            assert_eq!(
                directional_pose_scale_x(authored_facing, BodyFacing::Left),
                None
            );
            assert_eq!(
                directional_pose_scale_x(authored_facing, BodyFacing::Right),
                None
            );
        }
    }

    #[test]
    fn neutral_head_target_supports_cardinal_and_diagonal_movement() {
        let rest = Vec2::new(0.25, 1.0);
        assert_eq!(
            neutral_head_target(
                rest,
                AuthoredFacing::Neutral,
                MovementDirection::new(-1.0, 0.0),
                0.06,
            ),
            Some(Vec2::new(0.19, 1.0))
        );

        let diagonal = neutral_head_target(
            rest,
            AuthoredFacing::Neutral,
            MovementDirection::new(1.0, 1.0),
            0.06,
        )
        .expect("neutral pose has a head target");
        assert!((diagonal.distance(rest) - 0.06).abs() < EPSILON);
        assert!(diagonal.x > rest.x && diagonal.y > rest.y);
    }

    #[test]
    fn neutral_head_returns_to_rest_when_movement_stops() {
        let rest = Vec2::new(0.0, 1.25);
        assert_eq!(
            neutral_head_target(rest, AuthoredFacing::Neutral, MovementDirection::ZERO, 0.06,),
            Some(rest)
        );
    }

    #[test]
    fn directional_top_and_down_assets_do_not_receive_neutral_head_motion() {
        for authored_facing in [
            AuthoredFacing::Left,
            AuthoredFacing::Right,
            AuthoredFacing::Top,
            AuthoredFacing::Down,
        ] {
            assert_eq!(
                neutral_head_target(
                    Vec2::ZERO,
                    authored_facing,
                    MovementDirection::new(1.0, 0.0),
                    0.06,
                ),
                None
            );
        }
    }

    #[test]
    fn replicated_pose_flips_only_the_visual_orientation_root() {
        let mut app = App::new();
        app.add_systems(Update, apply_body_facing);
        let orientation_root = app
            .world_mut()
            .spawn((CharacterVisualOrientation, Transform::default()))
            .id();
        let player = app
            .world_mut()
            .spawn((
                BodyFacing::Right,
                CharacterVisual {
                    body_pivot: Vec2::ZERO,
                    orientation_root,
                    authored_facing: AuthoredFacing::Left,
                    outline_visuals: Vec::new(),
                },
                Transform::from_scale(Vec3::splat(2.0)),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<Transform>(orientation_root)
                .map(|transform| transform.scale),
            Some(Vec3::new(-1.0, 1.0, 1.0))
        );
        assert_eq!(
            app.world()
                .get::<Transform>(player)
                .map(|transform| transform.scale),
            Some(Vec3::splat(2.0))
        );
    }

    #[test]
    fn death_confirmation_rotation_keeps_the_body_pivot_fixed() {
        let design = game01_configs::load_embedded().expect("embedded design is valid");
        let rules = CharacterLifeRules::from_design(&design).expect("life rules are valid");
        let mut app = App::new();
        app.insert_resource(rules)
            .add_systems(Update, apply_character_status_presentation);

        let orientation_root = app
            .world_mut()
            .spawn((CharacterVisualOrientation, Transform::default()))
            .id();
        let body_pivot = Vec2::new(0.2, 0.8);
        app.world_mut().spawn((
            CharacterVisual {
                body_pivot,
                orientation_root,
                authored_facing: AuthoredFacing::Neutral,
                outline_visuals: Vec::new(),
            },
            CharacterLifeState::DeathConfirming,
            DeathConfirmationState { held_ticks: 60 },
            StatusEffectState::default(),
        ));

        app.update();

        let transform = app
            .world()
            .get::<Transform>(orientation_root)
            .expect("orientation root has a transform");
        let transformed_pivot = transform.transform_point(body_pivot.extend(0.0));
        assert!(transformed_pivot.truncate().distance(body_pivot) < 0.000_001);
    }
}

use std::f32::consts::PI;

use bevy::{
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d},
};
use game01_simulation::HammerAttackRules;
use game01_world_data::{GazeDirection, HammerAttackPhase, HammerAttackState, WeaponAimState};

use crate::polytools::HammerVisual;

const APEX_SCALE: f32 = 1.25;
const IMPACT_SCALE: f32 = 1.0;
const EMBEDDED_SHAKE_METERS: f32 = 0.006;
const HAMMER_SHADER_PATH: &str = "shaders/hammer_presentation.wgsl";

#[derive(Debug, Clone, Copy, ShaderType)]
struct HammerPresentationUniform {
    color: Vec4,
    shake_offset: Vec2,
    shake_pivot: Vec2,
    shake_extent: f32,
    authored_layer: f32,
    presentation_layer: f32,
    padding: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HammerPresentationMaterial {
    #[uniform(0)]
    uniform: HammerPresentationUniform,
}

impl HammerPresentationMaterial {
    pub fn from_color(color: Color, authored_layer: f32, presentation_layer: f32) -> Self {
        Self {
            uniform: HammerPresentationUniform {
                color: color.to_linear().to_vec4(),
                shake_offset: Vec2::ZERO,
                shake_pivot: Vec2::ZERO,
                shake_extent: 1.0,
                authored_layer,
                presentation_layer,
                padding: 0.0,
            },
        }
    }
}

impl Material2d for HammerPresentationMaterial {
    fn vertex_shader() -> ShaderRef {
        HAMMER_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        HAMMER_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

pub fn apply_hammer_pose(
    fixed_time: Res<Time<Fixed>>,
    rules: Res<HammerAttackRules>,
    players: Query<(&WeaponAimState, &HammerAttackState, &Transform), Without<HammerVisual>>,
    mut hammers: Query<(&HammerVisual, &mut Transform), With<HammerVisual>>,
    mut materials: ResMut<Assets<HammerPresentationMaterial>>,
) {
    let overstep = fixed_time.overstep_fraction();
    for (hammer, mut transform) in &mut hammers {
        let Ok((weapon_aim, attack, owner_transform)) = players.get(hammer.owner) else {
            continue;
        };
        let next_transform = hammer_pose(
            hammer,
            owner_transform.translation.truncate(),
            weapon_aim.direction(),
            *attack,
            rules.as_ref(),
            overstep,
        );
        let shake = match attack.phase {
            HammerAttackPhase::Embedded => embedded_shake(*attack, overstep),
            _ => Vec2::ZERO,
        };
        let grip_progress = rules.grip_progress(attack.charge_ticks as f32);
        let shake_pivot = owner_transform.translation.truncate() - hammer.owner_asset_pivot
            + hammer.rest_transform.translation.truncate();
        let shake_extent = (hammer.attack_point_from_grip
            - hammer.secondary_grip_from_primary * grip_progress)
            .length();
        for handle in &hammer.material_handles {
            if let Some(mut material) = materials.get_mut(handle) {
                material.uniform.shake_offset = shake;
                material.uniform.shake_pivot = shake_pivot;
                material.uniform.shake_extent = shake_extent.max(f32::EPSILON);
                material.uniform.presentation_layer =
                    owner_transform.translation.z + next_transform.translation.z;
            }
        }
        *transform = next_transform;
    }
}

fn hammer_pose(
    hammer: &HammerVisual,
    owner_position: Vec2,
    weapon_aim: GazeDirection,
    attack: HammerAttackState,
    rules: &HammerAttackRules,
    overstep: f32,
) -> Transform {
    match attack.phase {
        HammerAttackPhase::Idle => direction(weapon_aim)
            .map_or(hammer.rest_transform, |weapon_aim| {
                posed_transform(hammer, -weapon_aim, 1.0, 0.0, 0.0, hammer.behind_layer)
            }),
        HammerAttackPhase::Charging => {
            let charge_ticks = attack.charge_ticks as f32 + overstep.clamp(0.0, 1.0);
            direction(attack.direction).map_or(hammer.rest_transform, |aim| {
                posed_transform(
                    hammer,
                    -aim,
                    rules.charge_scale(charge_ticks),
                    rules.grip_progress(charge_ticks),
                    rules.inward_pull_ratio(charge_ticks),
                    hammer.behind_layer,
                )
            })
        }
        HammerAttackPhase::Swing => {
            let progress = phase_progress(attack.phase_ticks, rules.swing_ticks(), overstep);
            let Some(aim) = direction(attack.direction) else {
                return hammer.rest_transform;
            };
            let charge_ticks = attack.charge_ticks as f32;
            let scale = swing_scale(rules.charge_scale(charge_ticks), progress);
            let pull_ratio = rules.inward_pull_ratio(charge_ticks) * (1.0 - smoothstep(progress));
            depth_swing_transform(
                hammer,
                aim,
                scale,
                rules.grip_progress(charge_ticks),
                pull_ratio,
                progress,
            )
        }
        HammerAttackPhase::Embedded => embedded_transform(hammer, owner_position, attack, rules),
        HammerAttackPhase::Recovery => {
            let progress = smoothstep(phase_progress(
                attack.phase_ticks,
                rules.recovery_ticks(),
                overstep,
            ));
            let source = embedded_transform(hammer, owner_position, attack, rules);
            let target = direction(weapon_aim).map_or(hammer.rest_transform, |aim| {
                posed_transform(hammer, -aim, 1.0, 0.0, 0.0, hammer.behind_layer)
            });
            interpolate_transform(source, target, progress, hammer.front_layer)
        }
    }
}

fn depth_swing_transform(
    hammer: &HammerVisual,
    attack_direction: Vec2,
    scale: f32,
    grip_progress: f32,
    inward_pull_ratio: f32,
    progress: f32,
) -> Transform {
    let progress = progress.clamp(0.0, 1.0);
    let start_direction = -attack_direction;
    let source_angle = hammer.attack_point_from_grip.to_angle();
    let screen_alignment = Quat::from_rotation_z(start_direction.to_angle() - source_angle);
    let depth_axis = Vec3::new(-attack_direction.y, attack_direction.x, 0.0).normalize();
    let depth_rotation = Quat::from_axis_angle(depth_axis, PI * progress);
    let rotation = depth_rotation * screen_alignment;
    let effective_grip = hammer.secondary_grip_from_primary * grip_progress.clamp(0.0, 1.0);
    let rotated_grip = rotation.mul_vec3((effective_grip * scale).extend(0.0));
    let pull_distance = (hammer.attack_point_from_grip - hammer.secondary_grip_from_primary)
        .length()
        * inward_pull_ratio.clamp(0.0, 1.0);
    let projected_direction = depth_rotation.mul_vec3(start_direction.extend(0.0));
    let socket = hammer.rest_transform.translation;
    let layer = if progress < 0.5 {
        hammer.behind_layer
    } else {
        hammer.front_layer
    };

    Transform {
        translation: Vec3::new(
            socket.x - rotated_grip.x - projected_direction.x * pull_distance,
            socket.y - rotated_grip.y - projected_direction.y * pull_distance,
            layer,
        ),
        rotation,
        scale: Vec3::new(scale, scale, 1.0),
    }
}

fn posed_transform(
    hammer: &HammerVisual,
    attack_point_direction: Vec2,
    scale: f32,
    grip_progress: f32,
    inward_pull_ratio: f32,
    layer: f32,
) -> Transform {
    let mut transform = hammer.rest_transform;
    let source_angle = hammer.attack_point_from_grip.to_angle();
    transform.rotation = Quat::from_rotation_z(attack_point_direction.to_angle() - source_angle);
    transform.scale = Vec3::new(scale, scale, 1.0);
    let effective_grip = hammer.secondary_grip_from_primary * grip_progress.clamp(0.0, 1.0);
    let rotated_grip = transform
        .rotation
        .mul_vec3((effective_grip * scale).extend(0.0))
        .truncate();
    let pull_distance = (hammer.attack_point_from_grip - hammer.secondary_grip_from_primary)
        .length()
        * inward_pull_ratio.clamp(0.0, 1.0);
    let socket = hammer.rest_transform.translation.truncate();
    transform.translation.x = socket.x - rotated_grip.x - attack_point_direction.x * pull_distance;
    transform.translation.y = socket.y - rotated_grip.y - attack_point_direction.y * pull_distance;
    transform.translation.z = layer;
    transform
}

fn embedded_transform(
    hammer: &HammerVisual,
    owner_position: Vec2,
    attack: HammerAttackState,
    rules: &HammerAttackRules,
) -> Transform {
    let head = Vec2::new(attack.impact_point.x, attack.impact_point.y) - owner_position
        + hammer.owner_asset_pivot;
    let socket = hammer.rest_transform.translation.truncate();
    let head_to_socket = socket - head;
    let grip_progress = rules.grip_progress(attack.charge_ticks as f32);
    let effective_grip = hammer.secondary_grip_from_primary * grip_progress;
    let source_grip_to_head = hammer.attack_point_from_grip - effective_grip;
    if head_to_socket.length_squared() <= f32::EPSILON
        || source_grip_to_head.length_squared() <= f32::EPSILON
    {
        return hammer.rest_transform;
    }
    let desired_head_direction = -head_to_socket.normalize();
    let screen_alignment = Quat::from_rotation_z(
        (-desired_head_direction).to_angle() - source_grip_to_head.to_angle(),
    );
    let depth_axis =
        Vec3::new(-desired_head_direction.y, desired_head_direction.x, 0.0).normalize();
    let rotation = Quat::from_axis_angle(depth_axis, PI) * screen_alignment;
    let rotated_attack_point = rotation
        .mul_vec3(hammer.attack_point_from_grip.extend(0.0))
        .truncate();
    Transform::from_xyz(
        head.x - rotated_attack_point.x,
        head.y - rotated_attack_point.y,
        hammer.front_layer,
    )
    .with_rotation(rotation)
}

fn embedded_shake(attack: HammerAttackState, overstep: f32) -> Vec2 {
    let Some(aim) = direction(attack.direction) else {
        return Vec2::ZERO;
    };
    let perpendicular = Vec2::new(-aim.y, aim.x);
    let tick = attack.phase_ticks as f32 + overstep.clamp(0.0, 1.0);
    perpendicular * (tick * 0.9).sin() * EMBEDDED_SHAKE_METERS
}

fn interpolate_transform(
    source: Transform,
    target: Transform,
    progress: f32,
    active_layer: f32,
) -> Transform {
    let progress = progress.clamp(0.0, 1.0);
    let mut transform = Transform {
        translation: source.translation.lerp(target.translation, progress),
        rotation: source.rotation.slerp(target.rotation, progress),
        scale: source.scale.lerp(target.scale, progress),
    };
    transform.translation.z = if progress < 1.0 {
        active_layer
    } else {
        target.translation.z
    };
    transform
}

fn phase_progress(phase_ticks: u32, duration_ticks: u32, overstep: f32) -> f32 {
    (phase_ticks as f32 + overstep.clamp(0.0, 1.0)) / duration_ticks.max(1) as f32
}

fn direction(direction: GazeDirection) -> Option<Vec2> {
    let direction = Vec2::new(direction.x, direction.y);
    (direction.is_finite() && direction != Vec2::ZERO).then(|| direction.normalize())
}

fn swing_scale(start_scale: f32, progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.5 {
        start_scale + (APEX_SCALE - start_scale) * smoothstep(progress * 2.0)
    } else {
        APEX_SCALE + (IMPACT_SCALE - APEX_SCALE) * smoothstep((progress - 0.5) * 2.0)
    }
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hammer_visual() -> HammerVisual {
        HammerVisual {
            owner: Entity::PLACEHOLDER,
            rest_transform: Transform::from_xyz(2.0, 3.0, -0.2),
            attack_point_from_grip: Vec2::X,
            secondary_grip_from_primary: Vec2::new(-0.25, 0.0),
            owner_asset_pivot: Vec2::ZERO,
            material_handles: Vec::new(),
            behind_layer: -0.2,
            front_layer: 0.3,
        }
    }

    fn attack_rules() -> HammerAttackRules {
        let config = game01_configs::load_embedded().expect("embedded design config parses");
        HammerAttackRules::from_design(&config).expect("Hammer attack config is valid")
    }

    #[test]
    fn swing_scale_reaches_confirmed_key_scales() {
        assert_eq!(swing_scale(0.5, 0.0), 0.5);
        assert_eq!(swing_scale(0.5, 0.5), APEX_SCALE);
        assert_eq!(swing_scale(0.5, 1.0), IMPACT_SCALE);
    }

    #[test]
    fn depth_swing_projects_from_opposite_through_edge_on_to_impact() {
        let hammer = hammer_visual();
        let aim = Vec2::X;
        let start = depth_swing_transform(&hammer, aim, 1.0, 0.0, 0.0, 0.0);
        let edge_on = depth_swing_transform(&hammer, aim, 1.0, 0.0, 0.0, 0.5);
        let impact = depth_swing_transform(&hammer, aim, 1.0, 0.0, 0.0, 1.0);
        let projected_head = |transform: Transform| {
            transform
                .rotation
                .mul_vec3(hammer.attack_point_from_grip.extend(0.0))
        };

        assert!(projected_head(start).truncate().normalize().distance(-aim) < 0.000_01);
        assert!(projected_head(edge_on).truncate().length() < 0.000_01);
        assert!(projected_head(edge_on).z.abs() > 0.9);
        assert!(projected_head(impact).truncate().normalize().distance(aim) < 0.000_01);
        assert_eq!(start.translation.z, hammer.behind_layer);
        assert!((impact.translation.z - hammer.front_layer).abs() < 0.000_01);
    }

    #[test]
    fn idle_pose_points_authored_attack_point_opposite_the_weapon_aim() {
        let hammer = hammer_visual();
        let pose = hammer_pose(
            &hammer,
            Vec2::ZERO,
            GazeDirection::new(0.0, 1.0),
            HammerAttackState::IDLE,
            &attack_rules(),
            0.0,
        );
        let transformed_direction = pose
            .rotation
            .mul_vec3(hammer.attack_point_from_grip.extend(0.0))
            .truncate()
            .normalize();

        assert!(transformed_direction.distance(Vec2::NEG_Y) < 0.000_01);
        assert_eq!(pose.translation.z, hammer.behind_layer);
    }

    #[test]
    fn overhead_apex_switches_to_front_layer_and_largest_scale() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let pose = hammer_pose(
            &hammer,
            Vec2::ZERO,
            GazeDirection::new(0.0, 1.0),
            HammerAttackState {
                phase: HammerAttackPhase::Swing,
                direction: GazeDirection::new(0.0, 1.0),
                phase_ticks: 34,
                charge_ticks: 300,
                impact_point: game01_world_data::Position::ZERO,
            },
            &rules,
            0.5,
        );

        assert_eq!(pose.translation.z, hammer.front_layer);
        assert!((pose.scale.x - APEX_SCALE).abs() < 0.000_01);
        assert!((pose.scale.y - APEX_SCALE).abs() < 0.000_01);
    }

    #[test]
    fn charge_pose_uses_two_visual_scale_stages_without_changing_locked_grip() {
        let rules = attack_rules();

        assert!((rules.grip_progress(60.0) - 0.5).abs() < 0.000_01);
        assert!((rules.charge_scale(60.0) - 0.9).abs() < 0.000_01);
        assert!((rules.charge_scale(120.0) - 0.8).abs() < 0.000_01);
        assert!((rules.charge_scale(300.0) - 0.5).abs() < 0.000_01);
        assert_eq!(rules.inward_pull_ratio(120.0), 0.0);
        assert!((rules.inward_pull_ratio(300.0) - 0.05).abs() < 0.000_01);
    }

    #[test]
    fn embedded_pose_keeps_the_attack_point_at_its_world_anchor() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let owner = Vec2::new(4.0, -2.0);
        let impact = Vec2::new(7.0, 1.5);
        let attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: 0,
            charge_ticks: 120,
            impact_point: game01_world_data::Position::new(impact.x, impact.y),
        };
        let pose = embedded_transform(&hammer, owner, attack, &rules);
        let local_head = pose
            .to_matrix()
            .transform_point3(hammer.attack_point_from_grip.extend(0.0))
            .truncate();
        let world_head = owner - hammer.owner_asset_pivot + local_head;

        assert!(world_head.distance(impact) < 0.000_01);
    }

    #[test]
    fn depth_swing_and_embedded_pose_meet_without_an_impact_snap() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let aim = Vec2::X;
        let swing = depth_swing_transform(&hammer, aim, 1.0, 1.0, 0.0, 1.0);
        let impact = swing
            .to_matrix()
            .transform_point3(hammer.attack_point_from_grip.extend(0.0))
            .truncate();
        let attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: 0,
            charge_ticks: 120,
            impact_point: game01_world_data::Position::new(impact.x, impact.y),
        };
        let embedded = embedded_transform(&hammer, Vec2::ZERO, attack, &rules);

        assert!(
            swing
                .translation
                .truncate()
                .distance(embedded.translation.truncate())
                < 0.000_01
        );
        assert!(swing.rotation.dot(embedded.rotation).abs() > 0.999_99);
    }
}

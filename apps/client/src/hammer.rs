use std::f32::consts::PI;

use bevy::prelude::*;
use game01_simulation::HammerAttackRules;
use game01_world_data::{GazeDirection, HammerAttackPhase, HammerAttackState};

use crate::polytools::HammerVisual;

const CHARGING_SCALE: f32 = 0.7;
const APEX_SCALE: f32 = 1.25;
const IMPACT_SCALE: f32 = 1.0;

pub fn apply_hammer_pose(
    fixed_time: Res<Time<Fixed>>,
    rules: Res<HammerAttackRules>,
    players: Query<(&GazeDirection, &HammerAttackState)>,
    mut hammers: Query<(&HammerVisual, &mut Transform)>,
) {
    let overstep = fixed_time.overstep_fraction();
    for (hammer, mut transform) in &mut hammers {
        let Ok((gaze, attack)) = players.get(hammer.owner) else {
            continue;
        };
        *transform = hammer_pose(hammer, *gaze, *attack, rules.as_ref(), overstep);
    }
}

fn hammer_pose(
    hammer: &HammerVisual,
    gaze: GazeDirection,
    attack: HammerAttackState,
    rules: &HammerAttackRules,
    overstep: f32,
) -> Transform {
    match attack.phase {
        HammerAttackPhase::Idle => direction(gaze).map_or(hammer.rest_transform, |gaze| {
            posed_transform(hammer, -gaze, 1.0, hammer.behind_layer)
        }),
        HammerAttackPhase::Charging => direction(attack.direction)
            .map_or(hammer.rest_transform, |aim| {
                posed_transform(hammer, -aim, CHARGING_SCALE, hammer.behind_layer)
            }),
        HammerAttackPhase::Swing => {
            let progress = phase_progress(attack.phase_ticks, rules.swing_ticks(), overstep);
            let Some(aim) = direction(attack.direction) else {
                return hammer.rest_transform;
            };
            let angle = (-aim).to_angle() - PI * progress;
            let scale = swing_scale(progress);
            let layer = if progress < 0.5 {
                hammer.behind_layer
            } else {
                hammer.front_layer
            };
            posed_transform(hammer, Vec2::from_angle(angle), scale, layer)
        }
        HammerAttackPhase::Recovery => {
            let progress = phase_progress(attack.phase_ticks, rules.recovery_ticks(), overstep);
            let Some(aim) = direction(attack.direction) else {
                return hammer.rest_transform;
            };
            let angle = aim.to_angle() - PI * progress;
            posed_transform(
                hammer,
                Vec2::from_angle(angle),
                IMPACT_SCALE,
                hammer.front_layer,
            )
        }
    }
}

fn posed_transform(
    hammer: &HammerVisual,
    attack_point_direction: Vec2,
    scale: f32,
    layer: f32,
) -> Transform {
    let mut transform = hammer.rest_transform;
    let source_angle = hammer.attack_point_from_grip.to_angle();
    transform.rotation = Quat::from_rotation_z(attack_point_direction.to_angle() - source_angle);
    transform.scale = Vec3::new(scale, scale, 1.0);
    transform.translation.z = layer;
    transform
}

fn phase_progress(phase_ticks: u32, duration_ticks: u32, overstep: f32) -> f32 {
    (phase_ticks as f32 + overstep.clamp(0.0, 1.0)) / duration_ticks.max(1) as f32
}

fn direction(direction: GazeDirection) -> Option<Vec2> {
    let direction = Vec2::new(direction.x, direction.y);
    (direction.is_finite() && direction != Vec2::ZERO).then(|| direction.normalize())
}

fn swing_scale(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.5 {
        CHARGING_SCALE + (APEX_SCALE - CHARGING_SCALE) * smoothstep(progress * 2.0)
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
        assert_eq!(swing_scale(0.0), CHARGING_SCALE);
        assert_eq!(swing_scale(0.5), APEX_SCALE);
        assert_eq!(swing_scale(1.0), IMPACT_SCALE);
    }

    #[test]
    fn clockwise_swing_runs_from_opposite_to_attack_direction() {
        let aim = Vec2::Y;
        let start = Vec2::from_angle((-aim).to_angle());
        let apex = Vec2::from_angle((-aim).to_angle() - PI * 0.5);
        let impact = Vec2::from_angle((-aim).to_angle() - PI);

        assert!(start.distance(-aim) < 0.000_01);
        assert!(apex.distance(Vec2::NEG_X) < 0.000_01);
        assert!(impact.distance(aim) < 0.000_01);
    }

    #[test]
    fn idle_pose_points_authored_attack_point_opposite_the_gaze() {
        let hammer = hammer_visual();
        let pose = hammer_pose(
            &hammer,
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
            GazeDirection::new(0.0, 1.0),
            HammerAttackState {
                phase: HammerAttackPhase::Swing,
                direction: GazeDirection::new(0.0, 1.0),
                phase_ticks: 13,
                charge_ticks: 300,
            },
            &rules,
            0.5,
        );

        assert_eq!(pose.translation.z, hammer.front_layer);
        assert!((pose.scale.x - APEX_SCALE).abs() < 0.000_01);
        assert!((pose.scale.y - APEX_SCALE).abs() < 0.000_01);
    }
}

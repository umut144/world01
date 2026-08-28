use std::f32::consts::{PI, TAU};

use bevy::{
    asset::{load_internal_asset, uuid_handle},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef},
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};
use game01_configs::DesignConfig;
use game01_simulation::HammerAttackRules;
use game01_world_data::{
    GazeDirection, HammerAttackPhase, HammerAttackState, WeaponAimState, WeaponTurnDirection,
};

use crate::polytools::HammerVisual;

const APEX_SCALE: f32 = 1.25;
const IMPACT_SCALE: f32 = 1.0;
const EMBEDDED_SHAKE_METERS: f32 = 0.006;
const EMBEDDED_UNROLL_SECONDS: f32 = 0.15;
const HEAD_SINGULARITY_ENTER_METERS: f32 = 0.02;
const HEAD_SINGULARITY_EXIT_METERS: f32 = 0.04;
const HEAD_SINGULARITY_RELEASE_RADIANS_PER_SECOND: f32 = 4.0 * PI;
const ANGLE_EPSILON: f32 = 0.000_01;
const HAMMER_SHADER_HANDLE: Handle<Shader> = uuid_handle!("41caa612-7608-4bb0-80c0-aa418ba2c56a");

pub struct HammerPresentationPlugin;

impl Plugin for HammerPresentationPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            HAMMER_SHADER_HANDLE,
            "../../../assets/shaders/hammer_presentation.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(Material2dPlugin::<HammerPresentationMaterial>::default());
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct HammerPresentationRules {
    attack: HammerAttackRules,
    embedded_unroll_ticks: u32,
    scale_at_full_reach: f32,
    scale_at_full_charge: f32,
    maximum_inward_pull_ratio: f32,
}

impl HammerPresentationRules {
    pub fn from_design(config: &DesignConfig, attack: HammerAttackRules) -> Option<Self> {
        config
            .hammer_attack
            .presentation_is_valid()
            .then_some(Self {
                attack,
                embedded_unroll_ticks: ((config.simulation.ticks_per_second as f32
                    * EMBEDDED_UNROLL_SECONDS)
                    .round() as u32)
                    .max(1),
                scale_at_full_reach: config.hammer_attack.scale_at_full_reach,
                scale_at_full_charge: config.hammer_attack.scale_at_full_charge,
                maximum_inward_pull_ratio: config.hammer_attack.maximum_inward_pull_ratio,
            })
    }

    fn grip_progress(self, charge_ticks: f32) -> f32 {
        self.attack.grip_progress(charge_ticks)
    }

    fn charge_scale(self, charge_ticks: f32) -> f32 {
        let maximum_charge_ticks = self.attack.maximum_charge_ticks();
        let grip_reach_ticks = self.attack.grip_reach_ticks();
        let charge_ticks = charge_ticks.clamp(0.0, maximum_charge_ticks as f32);
        if charge_ticks <= grip_reach_ticks as f32 {
            let progress = charge_ticks / grip_reach_ticks as f32;
            return 1.0 + (self.scale_at_full_reach - 1.0) * progress;
        }
        let remaining = maximum_charge_ticks.saturating_sub(grip_reach_ticks).max(1);
        let progress = (charge_ticks - grip_reach_ticks as f32).clamp(0.0, remaining as f32)
            / remaining as f32;
        self.scale_at_full_reach + (self.scale_at_full_charge - self.scale_at_full_reach) * progress
    }

    fn inward_pull_ratio(self, charge_ticks: f32) -> f32 {
        let grip_reach_ticks = self.attack.grip_reach_ticks();
        if charge_ticks <= grip_reach_ticks as f32 {
            return 0.0;
        }
        let remaining = self
            .attack
            .maximum_charge_ticks()
            .saturating_sub(grip_reach_ticks)
            .max(1);
        let progress = (charge_ticks - grip_reach_ticks as f32).clamp(0.0, remaining as f32)
            / remaining as f32;
        self.maximum_inward_pull_ratio * progress
    }

    fn swing_ticks(self) -> u32 {
        self.attack.swing_ticks()
    }

    fn recovery_ticks(self) -> u32 {
        self.attack.recovery_ticks()
    }

    fn embedded_ticks(self) -> u32 {
        self.attack.embedded_ticks()
    }
}

#[derive(Component, Debug, Default)]
pub struct HammerPresentationState {
    previous_phase: Option<HammerAttackPhase>,
    previous_owner_position: Option<Vec2>,
    head_direction: HeadDirectionState,
    recovery: Option<HammerRecoveryVisualState>,
}

#[derive(Debug, Default)]
struct HeadDirectionState {
    angle: Option<f32>,
    singularity_latched: bool,
    releasing: bool,
}

#[derive(Debug, Clone, Copy)]
struct HammerRecoveryVisualState {
    source_world_translation: Vec2,
    source_angle: f32,
    source_scale: Vec3,
    target_angle: f32,
}

#[derive(Debug, Clone, Copy, ShaderType)]
struct HammerPresentationUniform {
    color: Vec4,
    shake_offset: Vec2,
    shake_pivot: Vec2,
    shake_extent: f32,
    authored_layer: f32,
    presentation_layer: f32,
    projection_depth_meters: f32,
    padding: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HammerPresentationMaterial {
    #[uniform(0)]
    uniform: HammerPresentationUniform,
}

impl HammerPresentationMaterial {
    pub fn from_color(
        color: Color,
        authored_layer: f32,
        presentation_layer: f32,
        projection_depth_meters: f32,
    ) -> Self {
        Self {
            uniform: HammerPresentationUniform {
                color: color.to_linear().to_vec4(),
                shake_offset: Vec2::ZERO,
                shake_pivot: Vec2::ZERO,
                shake_extent: 1.0,
                authored_layer,
                presentation_layer,
                projection_depth_meters,
                padding: 0.0,
            },
        }
    }
}

impl Material2d for HammerPresentationMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(HAMMER_SHADER_HANDLE.clone())
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(HAMMER_SHADER_HANDLE.clone())
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

pub fn apply_hammer_pose(
    fixed_time: Res<Time<Fixed>>,
    virtual_time: Res<Time<Virtual>>,
    rules: Res<HammerPresentationRules>,
    players: Query<(&WeaponAimState, &HammerAttackState, &Transform), Without<HammerVisual>>,
    mut hammers: Query<
        (&HammerVisual, &mut HammerPresentationState, &mut Transform),
        With<HammerVisual>,
    >,
    mut visual_visibility: Query<&mut Visibility, Without<HammerVisual>>,
    mut materials: ResMut<Assets<HammerPresentationMaterial>>,
) {
    let overstep = fixed_time.overstep_fraction();
    let delta_seconds = virtual_time.delta_secs();
    for (hammer, mut presentation, mut transform) in &mut hammers {
        let Ok((weapon_aim, attack, owner_transform)) = players.get(hammer.owner) else {
            continue;
        };
        let next_transform = hammer_pose(
            hammer,
            owner_transform.translation.truncate(),
            *weapon_aim,
            *attack,
            rules.as_ref(),
            overstep,
            delta_seconds,
            *transform,
            &mut presentation,
        );
        let shake = match attack.phase {
            HammerAttackPhase::Embedded => embedded_shake(*attack, overstep, rules.as_ref()),
            _ => Vec2::ZERO,
        };
        let uses_depth = uses_depth_visual(attack.phase);
        set_visual_visibility(&hammer.flat_visuals, !uses_depth, &mut visual_visibility);
        set_visual_visibility(
            &hammer.swing_depth_visuals,
            uses_depth,
            &mut visual_visibility,
        );
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

fn uses_depth_visual(phase: HammerAttackPhase) -> bool {
    matches!(
        phase,
        HammerAttackPhase::Swing | HammerAttackPhase::Embedded
    )
}

fn hammer_pose(
    hammer: &HammerVisual,
    owner_position: Vec2,
    weapon_aim: WeaponAimState,
    attack: HammerAttackState,
    rules: &HammerPresentationRules,
    overstep: f32,
    delta_seconds: f32,
    current_transform: Transform,
    presentation: &mut HammerPresentationState,
) -> Transform {
    let previous_phase = presentation.previous_phase;
    let next_transform = match attack.phase {
        HammerAttackPhase::Idle => direction(weapon_aim.direction())
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
            direction(attack.direction).map_or(hammer.rest_transform, |aim| {
                let charge_ticks = attack.charge_ticks as f32;
                let scale = swing_scale(rules.charge_scale(charge_ticks), progress);
                let pull_ratio =
                    rules.inward_pull_ratio(charge_ticks) * (1.0 - smoothstep(progress));
                depth_swing_transform(
                    hammer,
                    aim,
                    scale,
                    rules.grip_progress(charge_ticks),
                    pull_ratio,
                    progress,
                )
            })
        }
        HammerAttackPhase::Embedded => {
            if previous_phase != Some(HammerAttackPhase::Embedded) {
                presentation.head_direction = HeadDirectionState::default();
                presentation.recovery = None;
            }
            embedded_transform(
                hammer,
                owner_position,
                attack,
                rules,
                overstep,
                delta_seconds,
                &mut presentation.head_direction,
            )
        }
        HammerAttackPhase::Recovery => recovery_transform(
            hammer,
            owner_position,
            weapon_aim,
            attack,
            rules,
            overstep,
            current_transform,
            previous_phase,
            presentation,
        ),
    };
    if !matches!(
        attack.phase,
        HammerAttackPhase::Embedded | HammerAttackPhase::Recovery
    ) {
        presentation.recovery = None;
    }
    presentation.previous_phase = Some(attack.phase);
    presentation.previous_owner_position = Some(owner_position);
    next_transform
}

fn set_visual_visibility(
    entities: &[Entity],
    visible: bool,
    visibility: &mut Query<&mut Visibility, Without<HammerVisual>>,
) {
    let next = if visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for entity in entities {
        if let Ok(mut current) = visibility.get_mut(*entity)
            && *current != next
        {
            *current = next;
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
    rules: &HammerPresentationRules,
    overstep: f32,
    delta_seconds: f32,
    head_state: &mut HeadDirectionState,
) -> Transform {
    let head = Vec2::new(attack.impact_point.x, attack.impact_point.y) - owner_position
        + hammer.owner_asset_pivot;
    let socket = hammer.rest_transform.translation.truncate();
    let grip_progress = rules.grip_progress(attack.charge_ticks as f32);
    let effective_grip = hammer.secondary_grip_from_primary * grip_progress;
    let source_grip_to_head = hammer.attack_point_from_grip - effective_grip;
    if source_grip_to_head.length_squared() <= f32::EPSILON {
        return hammer.rest_transform;
    }
    let fallback_direction = direction(attack.direction).unwrap_or(Vec2::X);
    let desired_head_direction =
        tracked_head_direction(head - socket, fallback_direction, delta_seconds, head_state);
    let screen_alignment = Quat::from_rotation_z(
        (-desired_head_direction).to_angle() - source_grip_to_head.to_angle(),
    );
    let depth_axis =
        Vec3::new(-desired_head_direction.y, desired_head_direction.x, 0.0).normalize();
    let embedded_rotation = Quat::from_axis_angle(depth_axis, PI) * screen_alignment;
    let unroll = smoothstep(embedded_unroll_progress(attack, overstep, rules));
    let longitudinal_roll = Quat::from_axis_angle(desired_head_direction.extend(0.0), PI * unroll);
    let rotation = longitudinal_roll * embedded_rotation;
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

fn planar_embedded_transform(
    hammer: &HammerVisual,
    head: Vec2,
    desired_head_direction: Vec2,
    source_grip_to_head: Vec2,
) -> Transform {
    let rotation =
        Quat::from_rotation_z(desired_head_direction.to_angle() - source_grip_to_head.to_angle());
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

fn recovery_transform(
    hammer: &HammerVisual,
    owner_position: Vec2,
    weapon_aim: WeaponAimState,
    attack: HammerAttackState,
    rules: &HammerPresentationRules,
    overstep: f32,
    current_transform: Transform,
    previous_phase: Option<HammerAttackPhase>,
    presentation: &mut HammerPresentationState,
) -> Transform {
    let target = carried_transform(hammer, weapon_aim.direction());
    let target_angle = planar_rotation_angle(target.rotation)
        .unwrap_or_else(|| planar_rotation_angle(hammer.rest_transform.rotation).unwrap_or(0.0));

    if previous_phase != Some(HammerAttackPhase::Recovery) || presentation.recovery.is_none() {
        let (source, source_owner_position) = if previous_phase == Some(HammerAttackPhase::Embedded)
        {
            (
                current_transform,
                presentation
                    .previous_owner_position
                    .unwrap_or(owner_position),
            )
        } else {
            (
                recovery_source_for_unsampled_entry(
                    hammer,
                    owner_position,
                    attack,
                    rules,
                    &presentation.head_direction,
                ),
                owner_position,
            )
        };
        let source_angle = planar_rotation_angle(source.rotation).unwrap_or(target_angle);
        presentation.recovery = Some(HammerRecoveryVisualState {
            source_world_translation: local_to_world_translation(
                hammer,
                source_owner_position,
                source.translation.truncate(),
            ),
            source_angle,
            source_scale: source.scale,
            target_angle: unwrap_angle_with_tie(
                source_angle,
                target_angle,
                weapon_aim.last_turn_direction,
            ),
        });
    }

    let Some(recovery) = presentation.recovery.as_mut() else {
        return target;
    };
    recovery.target_angle = unwrap_angle_near(recovery.target_angle, target_angle);
    let progress = smoothstep(phase_progress(
        attack.phase_ticks,
        rules.recovery_ticks(),
        overstep,
    ));
    let target_world_translation =
        local_to_world_translation(hammer, owner_position, target.translation.truncate());
    let world_translation = recovery
        .source_world_translation
        .lerp(target_world_translation, progress);
    let local_translation = world_to_local_translation(hammer, owner_position, world_translation);
    let angle = recovery.source_angle
        + (recovery.target_angle - recovery.source_angle) * progress.clamp(0.0, 1.0);
    let layer = if progress < 1.0 {
        hammer.front_layer
    } else {
        hammer.behind_layer
    };

    Transform {
        translation: local_translation.extend(layer),
        rotation: Quat::from_rotation_z(angle),
        scale: recovery.source_scale.lerp(target.scale, progress),
    }
}

fn recovery_source_for_unsampled_entry(
    hammer: &HammerVisual,
    owner_position: Vec2,
    attack: HammerAttackState,
    rules: &HammerPresentationRules,
    head_state: &HeadDirectionState,
) -> Transform {
    let head = Vec2::new(attack.impact_point.x, attack.impact_point.y) - owner_position
        + hammer.owner_asset_pivot;
    let socket = hammer.rest_transform.translation.truncate();
    let fallback = direction(attack.direction).unwrap_or(Vec2::X);
    let desired_head_direction = head_state.angle.map_or_else(
        || {
            let raw = head - socket;
            (raw.is_finite() && raw.length_squared() > f32::EPSILON)
                .then(|| raw.normalize())
                .unwrap_or(fallback)
        },
        Vec2::from_angle,
    );
    let effective_grip =
        hammer.secondary_grip_from_primary * rules.grip_progress(attack.charge_ticks as f32);
    let source_grip_to_head = hammer.attack_point_from_grip - effective_grip;
    if source_grip_to_head.length_squared() <= f32::EPSILON {
        return hammer.rest_transform;
    }
    planar_embedded_transform(hammer, head, desired_head_direction, source_grip_to_head)
}

fn carried_transform(hammer: &HammerVisual, weapon_aim: GazeDirection) -> Transform {
    direction(weapon_aim).map_or(hammer.rest_transform, |aim| {
        posed_transform(hammer, -aim, 1.0, 0.0, 0.0, hammer.behind_layer)
    })
}

fn embedded_shake(
    attack: HammerAttackState,
    overstep: f32,
    rules: &HammerPresentationRules,
) -> Vec2 {
    let Some(aim) = direction(attack.direction) else {
        return Vec2::ZERO;
    };
    let perpendicular = Vec2::new(-aim.y, aim.x);
    let tick = attack.phase_ticks as f32 + overstep.clamp(0.0, 1.0);
    let fade = 1.0 - smoothstep(embedded_unroll_progress(attack, overstep, rules));
    perpendicular * (tick * 0.9).sin() * EMBEDDED_SHAKE_METERS * fade
}

fn embedded_unroll_progress(
    attack: HammerAttackState,
    overstep: f32,
    rules: &HammerPresentationRules,
) -> f32 {
    let end_tick = rules.embedded_ticks().saturating_sub(1) as f32;
    let start_tick = (end_tick - rules.embedded_unroll_ticks as f32).max(0.0);
    let duration = (end_tick - start_tick).max(1.0);
    let tick = attack.phase_ticks as f32 + overstep.clamp(0.0, 1.0);
    ((tick - start_tick) / duration).clamp(0.0, 1.0)
}

fn tracked_head_direction(
    raw_direction: Vec2,
    fallback_direction: Vec2,
    delta_seconds: f32,
    state: &mut HeadDirectionState,
) -> Vec2 {
    let fallback_angle = fallback_direction.to_angle();
    let current_angle = state.angle.get_or_insert(fallback_angle);
    if !raw_direction.is_finite() {
        return Vec2::from_angle(*current_angle);
    }

    let distance = raw_direction.length();
    if state.singularity_latched {
        if distance < HEAD_SINGULARITY_EXIT_METERS {
            return Vec2::from_angle(*current_angle);
        }
        state.singularity_latched = false;
        state.releasing = true;
    } else if distance <= HEAD_SINGULARITY_ENTER_METERS {
        state.singularity_latched = true;
        state.releasing = false;
        return Vec2::from_angle(*current_angle);
    }

    if distance <= f32::EPSILON {
        return Vec2::from_angle(*current_angle);
    }
    let raw_angle = unwrap_angle_near(*current_angle, raw_direction.to_angle());
    if state.releasing {
        let maximum_step =
            (HEAD_SINGULARITY_RELEASE_RADIANS_PER_SECOND * delta_seconds.max(0.0)).min(PI / 6.0);
        let delta = raw_angle - *current_angle;
        if delta.abs() <= maximum_step.max(ANGLE_EPSILON) {
            *current_angle = raw_angle;
            state.releasing = false;
        } else {
            *current_angle += delta.signum() * maximum_step;
        }
    } else {
        *current_angle = raw_angle;
    }
    Vec2::from_angle(*current_angle)
}

fn local_to_world_translation(
    hammer: &HammerVisual,
    owner_position: Vec2,
    local_translation: Vec2,
) -> Vec2 {
    owner_position - hammer.owner_asset_pivot + local_translation
}

fn world_to_local_translation(
    hammer: &HammerVisual,
    owner_position: Vec2,
    world_translation: Vec2,
) -> Vec2 {
    world_translation - owner_position + hammer.owner_asset_pivot
}

fn planar_rotation_angle(rotation: Quat) -> Option<f32> {
    let projected_x = rotation.mul_vec3(Vec3::X).truncate();
    (projected_x.is_finite() && projected_x.length_squared() > f32::EPSILON)
        .then(|| projected_x.to_angle())
}

fn unwrap_angle_near(reference: f32, angle: f32) -> f32 {
    reference + (angle - reference + PI).rem_euclid(TAU) - PI
}

fn unwrap_angle_with_tie(reference: f32, angle: f32, tie_direction: WeaponTurnDirection) -> f32 {
    let candidate = unwrap_angle_near(reference, angle);
    let delta = candidate - reference;
    if (delta.abs() - PI).abs() <= ANGLE_EPSILON {
        reference + tie_direction.angle_sign() * PI
    } else {
        candidate
    }
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
            flat_visuals: Vec::new(),
            swing_depth_visuals: Vec::new(),
            behind_layer: -0.2,
            front_layer: 0.3,
        }
    }

    fn attack_rules() -> HammerPresentationRules {
        let config = game01_configs::load_embedded().expect("embedded design config parses");
        let attack = HammerAttackRules::from_design(&config).expect("Hammer attack config parses");
        HammerPresentationRules::from_design(&config, attack)
            .expect("Hammer presentation config parses")
    }

    fn weapon_aim(angle: f32, turn: WeaponTurnDirection) -> WeaponAimState {
        WeaponAimState::new(angle, turn)
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
        let mut presentation = HammerPresentationState::default();
        let pose = hammer_pose(
            &hammer,
            Vec2::ZERO,
            weapon_aim(PI * 0.5, WeaponTurnDirection::CounterClockwise),
            HammerAttackState::IDLE,
            &attack_rules(),
            0.0,
            1.0 / 60.0,
            hammer.rest_transform,
            &mut presentation,
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
        let mut presentation = HammerPresentationState::default();
        let pose = hammer_pose(
            &hammer,
            Vec2::ZERO,
            weapon_aim(PI * 0.5, WeaponTurnDirection::CounterClockwise),
            HammerAttackState {
                phase: HammerAttackPhase::Swing,
                direction: GazeDirection::new(0.0, 1.0),
                phase_ticks: 34,
                charge_ticks: 300,
                impact_point: game01_world_data::Position::ZERO,
            },
            &rules,
            0.5,
            1.0 / 60.0,
            hammer.rest_transform,
            &mut presentation,
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
        let pose = embedded_transform(
            &hammer,
            owner,
            attack,
            &rules,
            0.0,
            1.0 / 60.0,
            &mut HeadDirectionState::default(),
        );
        let local_head = pose
            .to_matrix()
            .transform_point3(hammer.attack_point_from_grip.extend(0.0))
            .truncate();
        let world_head = owner - hammer.owner_asset_pivot + local_head;

        assert!(world_head.distance(impact) < 0.000_01);
    }

    #[test]
    fn recovery_stays_in_the_screen_plane() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let mut presentation = HammerPresentationState::default();
        let embedded_attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: rules.embedded_ticks().saturating_sub(1),
            charge_ticks: 120,
            impact_point: game01_world_data::Position::new(3.0, 3.0),
        };
        let source = hammer_pose(
            &hammer,
            Vec2::ZERO,
            WeaponAimState::RIGHT,
            embedded_attack,
            &rules,
            0.0,
            1.0 / 60.0,
            hammer.rest_transform,
            &mut presentation,
        );
        let midpoint = hammer_pose(
            &hammer,
            Vec2::ZERO,
            WeaponAimState::RIGHT,
            HammerAttackState {
                phase: HammerAttackPhase::Recovery,
                phase_ticks: rules.recovery_ticks() / 2,
                ..embedded_attack
            },
            &rules,
            0.0,
            1.0 / 60.0,
            source,
            &mut presentation,
        );

        for transform in [source, midpoint] {
            assert!(transform.rotation.mul_vec3(Vec3::X).z.abs() < 0.000_01);
            assert!(transform.rotation.mul_vec3(Vec3::Y).z.abs() < 0.000_01);
        }
    }

    #[test]
    fn depth_visual_lasts_through_embedded_shake_only() {
        assert!(!uses_depth_visual(HammerAttackPhase::Idle));
        assert!(!uses_depth_visual(HammerAttackPhase::Charging));
        assert!(uses_depth_visual(HammerAttackPhase::Swing));
        assert!(uses_depth_visual(HammerAttackPhase::Embedded));
        assert!(!uses_depth_visual(HammerAttackPhase::Recovery));
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
        let embedded = embedded_transform(
            &hammer,
            Vec2::ZERO,
            attack,
            &rules,
            0.0,
            1.0 / 60.0,
            &mut HeadDirectionState::default(),
        );

        assert!(
            swing
                .translation
                .truncate()
                .distance(embedded.translation.truncate())
                < 0.000_01
        );
        assert!(swing.rotation.dot(embedded.rotation).abs() > 0.999_99);
    }

    #[test]
    fn embedded_unroll_matches_flat_vertices_before_recovery() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: rules.embedded_ticks().saturating_sub(1),
            charge_ticks: 120,
            impact_point: game01_world_data::Position::new(3.0, 3.0),
        };
        let embedded = embedded_transform(
            &hammer,
            Vec2::ZERO,
            attack,
            &rules,
            0.0,
            1.0 / 60.0,
            &mut HeadDirectionState::default(),
        );
        let grip =
            hammer.secondary_grip_from_primary * rules.grip_progress(attack.charge_ticks as f32);
        let planar = planar_embedded_transform(
            &hammer,
            Vec2::new(attack.impact_point.x, attack.impact_point.y),
            Vec2::X,
            hammer.attack_point_from_grip - grip,
        );

        for point in [
            Vec3::new(0.2, 0.4, 0.0),
            Vec3::new(-0.3, 0.15, 0.0),
            hammer.attack_point_from_grip.extend(0.0),
        ] {
            let embedded_point = embedded.to_matrix().transform_point3(point);
            let planar_point = planar.to_matrix().transform_point3(point);
            assert!(embedded_point.distance(planar_point) < 0.000_01);
        }
    }

    #[test]
    fn recovery_keeps_one_arc_when_live_aim_crosses_the_half_turn_seam() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let mut presentation = HammerPresentationState::default();
        let embedded_attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: rules.embedded_ticks().saturating_sub(1),
            charge_ticks: 0,
            impact_point: game01_world_data::Position::new(3.0, 3.0),
        };
        let embedded = hammer_pose(
            &hammer,
            Vec2::ZERO,
            WeaponAimState::RIGHT,
            embedded_attack,
            &rules,
            0.0,
            1.0 / 60.0,
            hammer.rest_transform,
            &mut presentation,
        );
        let recovery_attack = HammerAttackState {
            phase: HammerAttackPhase::Recovery,
            phase_ticks: 21,
            ..embedded_attack
        };
        let before_seam = hammer_pose(
            &hammer,
            Vec2::ZERO,
            weapon_aim(0.0, WeaponTurnDirection::CounterClockwise),
            recovery_attack,
            &rules,
            0.0,
            1.0 / 60.0,
            embedded,
            &mut presentation,
        );
        let after_seam = hammer_pose(
            &hammer,
            Vec2::ZERO,
            weapon_aim(0.001, WeaponTurnDirection::CounterClockwise),
            recovery_attack,
            &rules,
            0.0,
            1.0 / 60.0,
            before_seam,
            &mut presentation,
        );
        let before_angle = planar_rotation_angle(before_seam.rotation).expect("planar recovery");
        let after_angle = unwrap_angle_near(
            before_angle,
            planar_rotation_angle(after_seam.rotation).expect("planar recovery"),
        );

        assert!((after_angle - before_angle).abs() < 0.001);
        assert!(before_angle > 0.5);
    }

    #[test]
    fn recovery_start_remains_world_fixed_while_the_owner_moves() {
        let hammer = hammer_visual();
        let rules = attack_rules();
        let mut presentation = HammerPresentationState::default();
        let embedded_attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: rules.embedded_ticks().saturating_sub(1),
            charge_ticks: 0,
            impact_point: game01_world_data::Position::new(3.0, 3.0),
        };
        let embedded = hammer_pose(
            &hammer,
            Vec2::ZERO,
            WeaponAimState::RIGHT,
            embedded_attack,
            &rules,
            0.0,
            1.0 / 60.0,
            hammer.rest_transform,
            &mut presentation,
        );
        let source_world =
            local_to_world_translation(&hammer, Vec2::ZERO, embedded.translation.truncate());
        let recovery = hammer_pose(
            &hammer,
            Vec2::new(0.75, -0.25),
            WeaponAimState::RIGHT,
            HammerAttackState {
                phase: HammerAttackPhase::Recovery,
                phase_ticks: 0,
                ..embedded_attack
            },
            &rules,
            0.0,
            1.0 / 60.0,
            embedded,
            &mut presentation,
        );
        let recovery_world = local_to_world_translation(
            &hammer,
            Vec2::new(0.75, -0.25),
            recovery.translation.truncate(),
        );

        assert!(source_world.distance(recovery_world) < 0.000_01);
    }

    #[test]
    fn planted_head_crossing_holds_then_releases_without_a_half_turn_snap() {
        let mut state = HeadDirectionState::default();
        let frame_seconds = 1.0 / 60.0;
        let initial =
            tracked_head_direction(Vec2::new(0.10, 0.0), Vec2::X, frame_seconds, &mut state);
        let entering =
            tracked_head_direction(Vec2::new(0.01, 0.0), Vec2::X, frame_seconds, &mut state);
        let crossed =
            tracked_head_direction(Vec2::new(-0.01, 0.0), Vec2::X, frame_seconds, &mut state);
        let exiting =
            tracked_head_direction(Vec2::new(-0.05, 0.0), Vec2::X, frame_seconds, &mut state);

        assert!(initial.distance(Vec2::X) < 0.000_01);
        assert!(entering.distance(initial) < 0.000_01);
        assert!(crossed.distance(initial) < 0.000_01);
        assert!(exiting.angle_to(initial).abs() <= 4.0 * PI / 60.0 + ANGLE_EPSILON);
        assert!(exiting.dot(initial) > 0.0);
    }

    #[test]
    fn embedded_shake_reaches_zero_before_the_flat_visual_swap() {
        let rules = attack_rules();
        let attack = HammerAttackState {
            phase: HammerAttackPhase::Embedded,
            direction: GazeDirection::RIGHT,
            phase_ticks: rules.embedded_ticks().saturating_sub(1),
            charge_ticks: 0,
            impact_point: game01_world_data::Position::ZERO,
        };

        assert_eq!(embedded_shake(attack, 0.0, &rules), Vec2::ZERO);
    }
}

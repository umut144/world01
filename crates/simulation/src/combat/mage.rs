use std::{error::Error, fmt};

use bevy::prelude::{Entity, MessageWriter, Query, Res, Resource, Vec2};
use world01_content::{
    AuthoredFacing, CharacterHurtGeometryCatalog, MageEyeGeometry, RuntimeComponentGeometry,
    WorldCollisionGeometryCatalog,
};
use world01_design::{MageDesign, MageEyeBeamsDesign};
use world01_world_data::{
    AttackIntent, BodyFacing, CharacterLifeState, DashState, EyeBeamState, GazeDirection,
    MageAttackPhase, MageAttackState, Position, SelectedCharacter, StatusEffectState,
};

use crate::condition::ActorCondition;
#[cfg(test)]
use crate::damage::apply_damage;
use crate::damage::{DamageDealt, DamageSource};

use super::overlap::{
    GeometryTransform, component_projection_minimum, components_overlap, hurt_transform,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MageAttackRules {
    minimum_charge_ticks: u32,
    maximum_charge_ticks: u32,
    forced_release_ticks: u32,
    cooldown_ticks: u32,
    speed_per_tick: f32,
    gaze_lock_ticks_per_charge_tick: f32,
    projectile_length_per_charge_tick: f32,
    range_per_charge_tick: f32,
    damage_per_charge_tick: f32,
    laser_width_to_eye_width_ratio: f32,
}

impl MageAttackRules {
    pub fn from_design(
        ticks_per_second: u32,
        mage_design: &MageDesign,
        attack_design: &MageEyeBeamsDesign,
    ) -> Result<Self, MageAttackConfigError> {
        if ticks_per_second == 0 || !mage_design.is_valid() || !attack_design.is_valid() {
            return Err(MageAttackConfigError);
        }
        let ticks = |seconds: f32| (seconds * ticks_per_second as f32).round().max(1.0) as u32;
        Ok(Self {
            minimum_charge_ticks: ticks(attack_design.minimum_charge_seconds),
            maximum_charge_ticks: ticks(attack_design.maximum_charge_seconds),
            forced_release_ticks: ticks(attack_design.forced_release_seconds),
            cooldown_ticks: ticks(attack_design.cooldown_seconds),
            speed_per_tick: attack_design.projectile_speed_meters_per_second
                / ticks_per_second as f32,
            gaze_lock_ticks_per_charge_tick: attack_design.gaze_lock_seconds_per_charge_second,
            projectile_length_per_charge_tick: attack_design.projectile_speed_meters_per_second
                * attack_design.gaze_lock_seconds_per_charge_second
                / ticks_per_second as f32,
            range_per_charge_tick: attack_design.range_meters_per_charge_second
                / ticks_per_second as f32,
            damage_per_charge_tick: attack_design.damage_per_charge_second
                / ticks_per_second as f32,
            laser_width_to_eye_width_ratio: mage_design.laser_width_to_eye_width_ratio,
        })
    }

    pub fn minimum_charge_ticks(self) -> u32 {
        self.minimum_charge_ticks
    }

    pub fn maximum_charge_ticks(self) -> u32 {
        self.maximum_charge_ticks
    }

    pub fn forced_release_ticks(self) -> u32 {
        self.forced_release_ticks
    }

    pub fn cooldown_ticks(self) -> u32 {
        self.cooldown_ticks
    }

    pub fn speed_per_tick(self) -> f32 {
        self.speed_per_tick
    }

    pub fn charge_progress(self, charge_ticks: u32) -> f32 {
        charge_ticks.min(self.maximum_charge_ticks) as f32 / self.maximum_charge_ticks as f32
    }

    fn gaze_lock_ticks(self, charge_ticks: u32) -> u32 {
        (charge_ticks.min(self.maximum_charge_ticks) as f32 * self.gaze_lock_ticks_per_charge_tick)
            .round()
            .max(1.0) as u32
    }

    fn projectile_values(self, charge_ticks: u32) -> (f32, f32, f32) {
        let charge_ticks = charge_ticks.min(self.maximum_charge_ticks) as f32;
        let range = charge_ticks * self.range_per_charge_tick;
        let damage = charge_ticks * self.damage_per_charge_tick;
        let length = charge_ticks * self.projectile_length_per_charge_tick;
        (range, damage, length)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MageAttackConfigError;

impl fmt::Display for MageAttackConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Mage eye-beam configuration is invalid")
    }
}

impl Error for MageAttackConfigError {}

pub fn advance_mage_attacks(
    rules: Option<Res<MageAttackRules>>,
    geometry: Option<Res<MageEyeGeometry>>,
    mut players: Query<(
        &AttackIntent,
        &GazeDirection,
        &Position,
        &BodyFacing,
        Option<&StatusEffectState>,
        Option<&CharacterLifeState>,
        &mut MageAttackState,
    )>,
) {
    let (Some(rules), Some(geometry)) = (rules, geometry) else {
        return;
    };
    for (attack, gaze, position, facing, status, life, mut state) in &mut players {
        let condition = ActorCondition::new(status, life);
        if condition.blocks_all_input() {
            *state = MageAttackState::IDLE;
            continue;
        }
        let action_pressed = attack.pressed && !condition.blocks_action_buttons();
        match state.phase {
            MageAttackPhase::Idle => {
                if action_pressed {
                    state.phase = MageAttackPhase::Charging;
                    state.phase_ticks = 1;
                    state.charge_ticks = 1;
                }
            }
            MageAttackPhase::Charging => {
                if !action_pressed {
                    if state.charge_ticks < rules.minimum_charge_ticks {
                        *state = MageAttackState::IDLE;
                    } else {
                        fire(
                            &rules, &geometry, *gaze, *position, *facing, false, &mut state,
                        );
                    }
                    continue;
                }
                let next = state.phase_ticks.saturating_add(1);
                state.phase_ticks = next;
                state.charge_ticks = next.min(rules.maximum_charge_ticks);
                if next >= rules.forced_release_ticks {
                    fire(
                        &rules, &geometry, *gaze, *position, *facing, true, &mut state,
                    );
                }
            }
            MageAttackPhase::Cooldown => {
                if state.release_required && !attack.pressed {
                    state.release_required = false;
                }
                state.phase_ticks = state
                    .phase_ticks
                    .saturating_add(1)
                    .min(rules.cooldown_ticks);
            }
            MageAttackPhase::WaitingForRelease => {
                if !attack.pressed {
                    *state = MageAttackState::IDLE;
                }
            }
        }
    }
}

fn fire(
    rules: &MageAttackRules,
    geometry: &MageEyeGeometry,
    gaze: GazeDirection,
    position: Position,
    facing: BodyFacing,
    release_required: bool,
    state: &mut MageAttackState,
) {
    let gaze = Vec2::new(gaze.x, gaze.y).normalize_or_zero();
    if gaze == Vec2::ZERO {
        *state = MageAttackState::IDLE;
        return;
    }
    let charge_ticks = state
        .charge_ticks
        .clamp(rules.minimum_charge_ticks, rules.maximum_charge_ticks);
    let (range_meters, damage_per_beam, projectile_length_meters) =
        rules.projectile_values(charge_ticks);
    let player_origin = Vec2::new(position.x, position.y);
    let mirrored = matches!(
        (geometry.authored_facing, facing),
        (AuthoredFacing::Left, BodyFacing::Right) | (AuthoredFacing::Right, BodyFacing::Left)
    );
    let emitter_origin = |emitter: world01_content::EyeBeamEmitterGeometry| {
        let offset = Vec2::new(
            if mirrored {
                -emitter.offset.x
            } else {
                emitter.offset.x
            },
            emitter.offset.y,
        );
        player_origin + offset
    };
    let left_origin = emitter_origin(geometry.left);
    let right_origin = emitter_origin(geometry.right);
    let beam = |emitter: world01_content::EyeBeamEmitterGeometry, origin: Vec2| EyeBeamState {
        origin: Position::new(origin.x, origin.y),
        direction: GazeDirection::new(gaze.x, gaze.y),
        width: emitter.width * rules.laser_width_to_eye_width_ratio,
        active: true,
    };
    *state = MageAttackState {
        phase: MageAttackPhase::Cooldown,
        phase_ticks: 0,
        charge_ticks,
        gaze_lock_ticks: rules.gaze_lock_ticks(charge_ticks),
        release_required,
        range_meters,
        projectile_length_meters,
        damage_per_beam,
        left_beam: beam(geometry.left, left_origin),
        right_beam: beam(geometry.right, right_origin),
    };
}

pub fn apply_mage_beam_damage(
    rules: Res<MageAttackRules>,
    hurt_geometry: Res<CharacterHurtGeometryCatalog>,
    world_collision: Res<WorldCollisionGeometryCatalog>,
    mut damage: MessageWriter<DamageDealt>,
    mut players: bevy::ecs::system::ParamSet<(
        Query<(Entity, &MageAttackState)>,
        Query<(
            Entity,
            &SelectedCharacter,
            &Position,
            &BodyFacing,
            &DashState,
            Option<&CharacterLifeState>,
        )>,
        Query<&mut MageAttackState>,
    )>,
) {
    let volleys = players
        .p0()
        .iter()
        .filter(|(_, state)| state.phase == MageAttackPhase::Cooldown)
        .map(|(entity, state)| (entity, *state))
        .collect::<Vec<_>>();

    for (owner, volley) in volleys {
        for left in [true, false] {
            let beam = if left {
                volley.left_beam
            } else {
                volley.right_beam
            };
            if !beam.active {
                continue;
            }
            let Some((segment, reached_range)) = beam_segment(&rules, &volley, beam) else {
                continue;
            };
            let beam_geometry = beam_geometry(segment.0, segment.1, beam.width);
            let beam_transform = GeometryTransform::IDENTITY;
            let origin = Vec2::new(beam.origin.x, beam.origin.y);
            let direction = Vec2::new(beam.direction.x, beam.direction.y);
            let mut best_distance = f32::INFINITY;
            let mut hit_target = None;
            let mut blocked = false;

            for region in &world_collision.regions {
                let transform = GeometryTransform::translated(region.position);
                if components_overlap(&beam_geometry, beam_transform, &region.component, transform)
                {
                    let distance = component_projection_minimum(
                        &region.component,
                        transform,
                        origin,
                        direction,
                    );
                    if distance < best_distance {
                        best_distance = distance;
                        hit_target = None;
                        blocked = true;
                    }
                }
            }

            {
                let mut targets = players.p1();
                for (entity, character, position, facing, dash, life) in &mut targets {
                    if entity == owner
                        || dash.invulnerable
                        || life.is_some_and(|life| !life.is_alive())
                    {
                        continue;
                    }
                    let Some(hurt) = hurt_geometry.character(&character.0) else {
                        continue;
                    };
                    let transform = hurt_transform(hurt, *position, *facing);
                    let mut distance = f32::INFINITY;
                    let hit = hurt.components.iter().any(|component| {
                        if !components_overlap(&beam_geometry, beam_transform, component, transform)
                        {
                            return false;
                        }
                        distance = distance.min(component_projection_minimum(
                            component, transform, origin, direction,
                        ));
                        true
                    });
                    if hit && distance < best_distance {
                        best_distance = distance;
                        hit_target = Some(entity);
                        blocked = false;
                    }
                }
            }

            if let Some(target) = hit_target {
                damage.write(DamageDealt {
                    target,
                    source: DamageSource::Actor(owner),
                    amount: volley.damage_per_beam,
                });
            }
            if blocked || hit_target.is_some() || reached_range {
                if let Ok(mut state) = players.p2().get_mut(owner) {
                    if left {
                        state.left_beam.active = false;
                    } else {
                        state.right_beam.active = false;
                    }
                }
            }
        }
    }
}

pub fn expire_mage_beams(rules: Res<MageAttackRules>, mut players: Query<&mut MageAttackState>) {
    for mut state in &mut players {
        if state.phase != MageAttackPhase::Cooldown {
            continue;
        }
        let distance = rules.speed_per_tick * state.phase_ticks as f32;
        if distance >= state.range_meters {
            state.left_beam.active = false;
            state.right_beam.active = false;
        }
    }
}

pub fn finish_mage_cooldowns(
    rules: Res<MageAttackRules>,
    mut players: Query<&mut MageAttackState>,
) {
    for mut state in &mut players {
        if state.phase != MageAttackPhase::Cooldown || state.phase_ticks < rules.cooldown_ticks {
            continue;
        }
        *state = if state.release_required {
            MageAttackState {
                phase: MageAttackPhase::WaitingForRelease,
                ..MageAttackState::IDLE
            }
        } else {
            MageAttackState::IDLE
        };
    }
}

pub fn visible_beam_segment(
    rules: &MageAttackRules,
    state: &MageAttackState,
    beam: EyeBeamState,
) -> Option<(Vec2, Vec2)> {
    beam_segment(rules, state, beam).map(|(segment, _)| segment)
}

fn beam_segment(
    rules: &MageAttackRules,
    state: &MageAttackState,
    beam: EyeBeamState,
) -> Option<((Vec2, Vec2), bool)> {
    if state.phase != MageAttackPhase::Cooldown || !beam.active || beam.width <= 0.0 {
        return None;
    }
    let origin = Vec2::new(beam.origin.x, beam.origin.y);
    let direction = Vec2::new(beam.direction.x, beam.direction.y).normalize_or_zero();
    let head = rules.speed_per_tick * state.phase_ticks as f32;
    if direction == Vec2::ZERO || head <= 0.0 || head > state.range_meters {
        return None;
    }
    let tail = (head - state.projectile_length_meters).max(0.0);
    Some((
        (origin + direction * tail, origin + direction * head),
        head >= state.range_meters,
    ))
}

fn beam_geometry(start: Vec2, end: Vec2, width: f32) -> RuntimeComponentGeometry {
    let perpendicular = (end - start).normalize_or_zero().perp() * width * 0.5;
    RuntimeComponentGeometry {
        component_id: "mage_eye_beam".into(),
        name: "mage_eye_beam".into(),
        vertices: vec![
            start + perpendicular,
            start - perpendicular,
            end - perpendicular,
            end + perpendicular,
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, IntoScheduleConfigs, Update};
    use world01_configs::load_embedded;
    use world01_content::{
        CharacterHurtGeometryCatalog, MageEyeGeometry, PlacedCollisionGeometry, RuntimeContent,
    };
    use world01_design::load_embedded as load_design;
    use world01_world_data::{CharacterHealth, CharacterId, StatusEffectState};

    fn rules() -> MageAttackRules {
        let config = load_embedded().expect("embedded config parses");
        let design = load_design().expect("embedded design parses");
        MageAttackRules::from_design(
            config.simulation.ticks_per_second,
            &design.mage,
            &design.mage_eye_beams,
        )
        .expect("Mage design is valid")
    }

    fn geometry() -> MageEyeGeometry {
        MageEyeGeometry::from_content(
            &RuntimeContent::load_embedded().expect("embedded content is valid"),
        )
        .expect("Mage eyes define beam emitters")
    }

    fn spawn_mage(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                SelectedCharacter(CharacterId("mage".into())),
                AttackIntent::PRESSED,
                GazeDirection::RIGHT,
                Position::ZERO,
                BodyFacing::Authored,
                StatusEffectState::default(),
                CharacterLifeState::Alive,
                MageAttackState::IDLE,
            ))
            .id()
    }

    #[test]
    fn linear_model_matches_minimum_and_maximum_values() {
        let rules = rules();
        assert_eq!(rules.minimum_charge_ticks(), 60);
        assert_eq!(rules.maximum_charge_ticks(), 120);
        assert_eq!(rules.forced_release_ticks(), 240);
        assert_eq!(rules.cooldown_ticks(), 120);
        assert_eq!(rules.gaze_lock_ticks(60), 6);
        assert_eq!(rules.gaze_lock_ticks(120), 12);
        assert_eq!(rules.projectile_values(60), (10.0, 10.0, 1.0));
        assert_eq!(rules.projectile_values(120), (20.0, 20.0, 2.0));
    }

    #[test]
    fn release_before_minimum_charge_is_a_feint() {
        let mut app = App::new();
        app.insert_resource(rules())
            .insert_resource(geometry())
            .add_systems(Update, advance_mage_attacks);
        let mage = spawn_mage(&mut app);
        for _ in 0..59 {
            app.update();
        }
        app.world_mut()
            .get_mut::<AttackIntent>(mage)
            .unwrap()
            .pressed = false;
        app.update();
        assert_eq!(
            *app.world().get::<MageAttackState>(mage).unwrap(),
            MageAttackState::IDLE
        );
    }

    #[test]
    fn minimum_charge_releases_the_ten_damage_ten_meter_volley() {
        let mut app = App::new();
        app.insert_resource(rules())
            .insert_resource(geometry())
            .add_systems(Update, advance_mage_attacks);
        let mage = spawn_mage(&mut app);
        for _ in 0..60 {
            app.update();
        }
        app.world_mut()
            .get_mut::<AttackIntent>(mage)
            .unwrap()
            .pressed = false;
        app.update();
        let state = *app.world().get::<MageAttackState>(mage).unwrap();
        assert_eq!(state.phase, MageAttackPhase::Cooldown);
        assert_eq!(state.charge_ticks, 60);
        assert_eq!(state.gaze_lock_ticks, 6);
        assert_eq!(state.range_meters, 10.0);
        assert_eq!(state.projectile_length_meters, 1.0);
        assert_eq!(state.damage_per_beam, 10.0);
        assert!(state.left_beam.active && state.right_beam.active);
        assert_eq!(state.left_beam.direction, GazeDirection::RIGHT);
        assert_eq!(state.right_beam.direction, GazeDirection::RIGHT);
    }

    #[test]
    fn laser_width_to_eye_width_ratio_scales_the_authored_beam_widths() {
        let config = load_embedded().expect("embedded config parses");
        let mut design = load_design().expect("embedded design parses");
        design.mage.laser_width_to_eye_width_ratio = 0.8;
        let rules = MageAttackRules::from_design(
            config.simulation.ticks_per_second,
            &design.mage,
            &design.mage_eye_beams,
        )
        .expect("Mage design is valid");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let geometry =
            MageEyeGeometry::from_content(&content).expect("Mage eyes define beam emitters");
        let mut volley = MageAttackState {
            phase: MageAttackPhase::Charging,
            charge_ticks: rules.minimum_charge_ticks(),
            ..MageAttackState::IDLE
        };

        fire(
            &rules,
            &geometry,
            GazeDirection::RIGHT,
            Position::ZERO,
            BodyFacing::Authored,
            false,
            &mut volley,
        );

        assert_eq!(volley.left_beam.width, geometry.left.width * 0.8);
        assert_eq!(volley.right_beam.width, geometry.right.width * 0.8);
    }

    #[test]
    fn held_attack_forces_release_then_waits_for_physical_release() {
        let mut app = App::new();
        app.insert_resource(rules())
            .insert_resource(geometry())
            .add_systems(
                Update,
                (
                    advance_mage_attacks,
                    expire_mage_beams,
                    finish_mage_cooldowns,
                )
                    .chain(),
            );
        let mage = spawn_mage(&mut app);
        for _ in 0..240 {
            app.update();
        }
        assert_eq!(
            app.world().get::<MageAttackState>(mage).unwrap().phase,
            MageAttackPhase::Cooldown
        );
        for _ in 0..120 {
            app.update();
        }
        assert_eq!(
            app.world().get::<MageAttackState>(mage).unwrap().phase,
            MageAttackPhase::WaitingForRelease
        );
        app.world_mut()
            .get_mut::<AttackIntent>(mage)
            .unwrap()
            .pressed = false;
        app.update();
        assert_eq!(
            app.world().get::<MageAttackState>(mage).unwrap().phase,
            MageAttackPhase::Idle
        );
    }

    #[test]
    fn stunned_aborts_a_committed_charge_without_firing() {
        let mut app = App::new();
        app.insert_resource(rules())
            .insert_resource(geometry())
            .add_systems(Update, advance_mage_attacks);
        let mage = spawn_mage(&mut app);
        for _ in 0..60 {
            app.update();
        }
        app.world_mut()
            .get_mut::<StatusEffectState>(mage)
            .unwrap()
            .stunned_ticks = 1;
        app.update();
        assert_eq!(
            *app.world().get::<MageAttackState>(mage).unwrap(),
            MageAttackState::IDLE
        );
    }

    #[test]
    fn gaze_remains_locked_until_the_emission_ticks_finish() {
        let mut app = App::new();
        app.add_systems(Update, crate::update_gaze_direction);
        let mage = app
            .world_mut()
            .spawn((
                world01_world_data::GazeIntent::new(0.0, 1.0),
                GazeDirection::RIGHT,
                CharacterLifeState::Alive,
                MageAttackState {
                    phase: MageAttackPhase::Cooldown,
                    phase_ticks: 5,
                    gaze_lock_ticks: 6,
                    ..MageAttackState::IDLE
                },
            ))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<GazeDirection>(mage).unwrap(),
            GazeDirection::RIGHT
        );
        app.world_mut()
            .get_mut::<MageAttackState>(mage)
            .unwrap()
            .phase_ticks = 6;
        app.update();
        assert_eq!(
            *app.world().get::<GazeDirection>(mage).unwrap(),
            GazeDirection::new(0.0, 1.0)
        );
    }

    #[test]
    fn both_parallel_beams_damage_a_target_in_their_shared_path() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let rules = rules();
        let geometry = geometry();
        let mut volley = MageAttackState {
            phase: MageAttackPhase::Charging,
            charge_ticks: 60,
            ..MageAttackState::IDLE
        };
        fire(
            &rules,
            &geometry,
            GazeDirection::RIGHT,
            Position::ZERO,
            BodyFacing::Authored,
            false,
            &mut volley,
        );
        volley.phase_ticks = 60;

        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(
                CharacterHurtGeometryCatalog::from_content(&content)
                    .expect("embedded hurt geometry is valid"),
            )
            .insert_resource(WorldCollisionGeometryCatalog { regions: vec![] })
            .add_message::<DamageDealt>()
            .add_systems(Update, (apply_mage_beam_damage, apply_damage).chain());
        let owner = app
            .world_mut()
            .spawn((SelectedCharacter(CharacterId("mage".into())), volley))
            .id();
        let target = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                Position::new(10.0, -0.3),
                BodyFacing::Authored,
                DashState::default(),
                CharacterLifeState::Alive,
                CharacterHealth::full(100.0),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<CharacterHealth>(target).unwrap().current,
            80.0
        );
        let state = app.world().get::<MageAttackState>(owner).unwrap();
        assert!(!state.left_beam.active && !state.right_beam.active);
    }

    #[test]
    fn collision_region_blocks_beams_before_a_hurt_region() {
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let rules = rules();
        let geometry = geometry();
        let mut volley = MageAttackState {
            phase: MageAttackPhase::Charging,
            charge_ticks: 60,
            ..MageAttackState::IDLE
        };
        fire(
            &rules,
            &geometry,
            GazeDirection::RIGHT,
            Position::ZERO,
            BodyFacing::Authored,
            false,
            &mut volley,
        );
        volley.phase_ticks = 60;
        let blocker = RuntimeComponentGeometry {
            component_id: "blocker".into(),
            name: "blocker".into(),
            vertices: vec![
                Vec2::new(9.25, -1.0),
                Vec2::new(9.75, -1.0),
                Vec2::new(9.75, 1.0),
                Vec2::new(9.25, 1.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        };

        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(
                CharacterHurtGeometryCatalog::from_content(&content)
                    .expect("embedded hurt geometry is valid"),
            )
            .insert_resource(WorldCollisionGeometryCatalog {
                regions: vec![PlacedCollisionGeometry {
                    instance_id: "blocker".into(),
                    position: Position::ZERO,
                    component: blocker,
                }],
            })
            .add_message::<DamageDealt>()
            .add_systems(Update, (apply_mage_beam_damage, apply_damage).chain());
        app.world_mut()
            .spawn((SelectedCharacter(CharacterId("mage".into())), volley));
        let target = app
            .world_mut()
            .spawn((
                SelectedCharacter(CharacterId("hammerer".into())),
                Position::new(10.0, -0.3),
                BodyFacing::Authored,
                DashState::default(),
                CharacterLifeState::Alive,
                CharacterHealth::full(100.0),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<CharacterHealth>(target).unwrap().current,
            100.0
        );
    }
}

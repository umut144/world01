use std::{error::Error, fmt};

use bevy::prelude::{Entity, Mut, ParamSet, Query, Res, Resource};
use world01_content::CharacterHurtGeometryCatalog;
use world01_design::HealthConfig;
use world01_world_data::{
    ActorId, Ankh, BodyFacing, CharacterHealth, CharacterId, CharacterLifeState, DashState,
    DeathConfirmIntent, DeathConfirmationState, HammerAttackState, MovementDirection,
    MovementVelocity, Position, RespawnState, RevivalState, RunState, SelectedCharacter,
    StatusEffectState,
};

use crate::respawn::{RespawnActor, choose_respawn_position};
use crate::spatial::overlap::{components_overlap, hurt_transform, posed_hurt_transform};

const DEAD_BODY_SCALE: f32 = 0.9;
const DEAD_BODY_ROTATION_RADIANS: f32 = -14.0_f32.to_radians();

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CharacterLifeRules {
    seconds_per_tick: f32,
    confirmation_duration_ticks: u32,
    revival_duration_ticks: u32,
    confirmation_initial_radians_per_second: f32,
    confirmation_angular_acceleration: f32,
    revival_health_ratio: f32,
    respawn_health_ratio: f32,
    ankh_respawn_radius_meters: f32,
}

impl CharacterLifeRules {
    pub fn from_design(
        ticks_per_second: u32,
        design: &HealthConfig,
    ) -> Result<Self, CharacterLifeConfigError> {
        if ticks_per_second == 0 || !design.is_valid() {
            return Err(CharacterLifeConfigError);
        }

        let seconds_per_tick = 1.0 / ticks_per_second as f32;
        let confirmation_duration = design.death_confirmation_seconds;
        let initial = design
            .death_confirmation_initial_degrees_per_second
            .to_radians();
        let maximum = design
            .death_confirmation_max_degrees_per_second
            .to_radians();
        let ticks = |seconds: f32| (seconds * ticks_per_second as f32).round().max(1.0) as u32;
        Ok(Self {
            seconds_per_tick,
            confirmation_duration_ticks: ticks(confirmation_duration),
            revival_duration_ticks: ticks(design.revival_seconds),
            confirmation_initial_radians_per_second: initial,
            confirmation_angular_acceleration: (maximum - initial) / confirmation_duration,
            revival_health_ratio: design.revival_health_percent / 100.0,
            respawn_health_ratio: design.respawn_health_percent / 100.0,
            ankh_respawn_radius_meters: design.ankh_respawn_radius_meters,
        })
    }

    pub fn confirmation_duration_ticks(self) -> u32 {
        self.confirmation_duration_ticks
    }

    pub fn revival_duration_ticks(self) -> u32 {
        self.revival_duration_ticks
    }

    pub fn confirmation_progress(self, held_ticks: f32) -> f32 {
        (held_ticks / self.confirmation_duration_ticks as f32).clamp(0.0, 1.0)
    }

    pub fn confirmation_angle_radians(self, held_ticks: f32) -> f32 {
        let seconds = held_ticks.max(0.0) * self.seconds_per_tick;
        self.confirmation_initial_radians_per_second * seconds
            + 0.5 * self.confirmation_angular_acceleration * seconds * seconds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterLifeConfigError;

impl fmt::Display for CharacterLifeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("character life configuration is invalid")
    }
}

impl Error for CharacterLifeConfigError {}

pub fn update_character_life(
    rules: Res<CharacterLifeRules>,
    hurt_geometry: Option<Res<CharacterHurtGeometryCatalog>>,
    ankhs: Query<(&Ankh, &Position), bevy::prelude::Without<ActorId>>,
    mut actors: ParamSet<(
        Query<(
            Entity,
            &ActorId,
            Option<&SelectedCharacter>,
            Option<&Position>,
            Option<&BodyFacing>,
            &CharacterLifeState,
            &DeathConfirmIntent,
            Option<&StatusEffectState>,
        )>,
        Query<(
            Entity,
            &ActorId,
            Option<&SelectedCharacter>,
            Option<&mut Position>,
            Option<&BodyFacing>,
            &mut CharacterHealth,
            &mut CharacterLifeState,
            &mut DeathConfirmationState,
            &mut RevivalState,
            &mut RespawnState,
            &DeathConfirmIntent,
            Option<&StatusEffectState>,
        )>,
        Query<(
            &CharacterLifeState,
            Option<&mut MovementVelocity>,
            Option<&mut MovementDirection>,
            Option<&mut RunState>,
            Option<&mut DashState>,
            Option<&mut HammerAttackState>,
        )>,
    )>,
) {
    let snapshots = actors
        .p0()
        .iter()
        .map(
            |(entity, actor_id, character, position, facing, life, death_confirm, status)| {
                LifeSnapshot {
                    entity,
                    actor_id: actor_id.0,
                    character: character.map(|character| character.0.clone()),
                    position: position.copied(),
                    facing: facing.copied(),
                    life: *life,
                    death_confirm: *death_confirm,
                    status: status.copied(),
                }
            },
        )
        .collect::<Vec<_>>();
    let respawn_actors = snapshots
        .iter()
        .map(|snapshot| RespawnActor {
            actor_id: snapshot.actor_id,
            character: snapshot.character.clone(),
            position: snapshot.position,
            facing: snapshot.facing,
            life: snapshot.life,
        })
        .collect::<Vec<_>>();
    let ankhs = ankhs
        .iter()
        .map(|(ankh, position)| (*ankh, *position))
        .collect::<Vec<_>>();
    for (
        entity,
        actor_id,
        character,
        mut position,
        facing,
        mut health,
        mut life,
        mut confirmation,
        mut revival,
        mut respawn,
        death_confirm,
        _status,
    ) in &mut actors.p1()
    {
        let maximum = health.maximum.max(0.0);
        health.maximum = maximum;
        health.current = health.current.clamp(0.0, maximum);

        match *life {
            CharacterLifeState::Alive if health.current <= 0.0 => {
                health.current = 0.0;
                *life = CharacterLifeState::Dead;
                confirmation.held_ticks = 0;
                *revival = RevivalState::IDLE;
            }
            CharacterLifeState::Alive => {
                confirmation.held_ticks = 0;
                *revival = RevivalState::IDLE;
            }
            CharacterLifeState::Dead => {
                health.current = 0.0;
                if death_confirm.pressed {
                    *life = CharacterLifeState::DeathConfirming;
                    advance_confirmation(
                        *rules,
                        &mut confirmation,
                        &mut revival,
                        &mut health,
                        &mut life,
                        *actor_id,
                        &mut respawn,
                        position.as_deref_mut(),
                        character.map(|character| &character.0),
                        facing.copied(),
                        &ankhs,
                        &respawn_actors,
                        hurt_geometry.as_deref(),
                    );
                } else {
                    confirmation.held_ticks = confirmation.held_ticks.saturating_sub(1);
                    if let Some(reviver_actor_id) = find_reviver(
                        entity,
                        character.map(|character| &character.0),
                        position.as_deref(),
                        facing,
                        hurt_geometry.as_deref(),
                        &snapshots,
                    ) {
                        *life = CharacterLifeState::Reviving;
                        *revival = RevivalState {
                            reviver_actor_id: Some(reviver_actor_id),
                            held_ticks: 0,
                        };
                    }
                }
            }
            CharacterLifeState::DeathConfirming => {
                health.current = 0.0;
                if death_confirm.pressed {
                    advance_confirmation(
                        *rules,
                        &mut confirmation,
                        &mut revival,
                        &mut health,
                        &mut life,
                        *actor_id,
                        &mut respawn,
                        position.as_deref_mut(),
                        character.map(|character| &character.0),
                        facing.copied(),
                        &ankhs,
                        &respawn_actors,
                        hurt_geometry.as_deref(),
                    );
                } else {
                    *life = CharacterLifeState::Dead;
                    confirmation.held_ticks = confirmation.held_ticks.saturating_sub(1);
                }
            }
            CharacterLifeState::Reviving => {
                health.current = 0.0;
                if death_confirm.pressed {
                    *life = CharacterLifeState::DeathConfirming;
                    advance_confirmation(
                        *rules,
                        &mut confirmation,
                        &mut revival,
                        &mut health,
                        &mut life,
                        *actor_id,
                        &mut respawn,
                        position.as_deref_mut(),
                        character.map(|character| &character.0),
                        facing.copied(),
                        &ankhs,
                        &respawn_actors,
                        hurt_geometry.as_deref(),
                    );
                } else if revival_continues(
                    revival.reviver_actor_id,
                    entity,
                    character.map(|character| &character.0),
                    position.as_deref(),
                    facing,
                    hurt_geometry.as_deref(),
                    &snapshots,
                ) {
                    revival.held_ticks = revival
                        .held_ticks
                        .saturating_add(1)
                        .min(rules.revival_duration_ticks());
                    if revival.held_ticks >= rules.revival_duration_ticks() {
                        health.current = health.maximum * rules.revival_health_ratio;
                        *life = CharacterLifeState::Alive;
                        confirmation.held_ticks = 0;
                        *revival = RevivalState::IDLE;
                    }
                } else {
                    *life = CharacterLifeState::Dead;
                    *revival = RevivalState::IDLE;
                }
            }
        }
    }

    for (life, velocity, direction, run, dash, attack) in &mut actors.p2() {
        if !life.is_alive() {
            clear_incapacitated_actions(velocity, direction, run, dash, attack);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_confirmation(
    rules: CharacterLifeRules,
    confirmation: &mut DeathConfirmationState,
    revival: &mut RevivalState,
    health: &mut CharacterHealth,
    life: &mut CharacterLifeState,
    actor_id: ActorId,
    respawn: &mut RespawnState,
    position: Option<&mut Position>,
    character: Option<&CharacterId>,
    facing: Option<BodyFacing>,
    ankhs: &[(Ankh, Position)],
    actors: &[RespawnActor],
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
) {
    confirmation.held_ticks = confirmation
        .held_ticks
        .saturating_add(1)
        .min(rules.confirmation_duration_ticks());
    *revival = RevivalState::IDLE;
    if confirmation.held_ticks >= rules.confirmation_duration_ticks() {
        health.current = health.maximum * rules.respawn_health_ratio;
        *life = CharacterLifeState::Alive;
        confirmation.held_ticks = 0;
        respawn.count = respawn.count.saturating_add(1);
        if let Some(position) = position {
            let fallback = *position;
            *position = choose_respawn_position(
                actor_id.0,
                respawn.count,
                fallback,
                character,
                facing,
                rules.ankh_respawn_radius_meters,
                ankhs,
                actors,
                hurt_geometry,
            );
        }
    }
}

fn clear_incapacitated_actions(
    velocity: Option<Mut<MovementVelocity>>,
    direction: Option<Mut<MovementDirection>>,
    run: Option<Mut<RunState>>,
    dash: Option<Mut<DashState>>,
    attack: Option<Mut<HammerAttackState>>,
) {
    if let Some(mut velocity) = velocity {
        *velocity = MovementVelocity::ZERO;
    }
    if let Some(mut direction) = direction {
        *direction = MovementDirection::ZERO;
    }
    if let Some(mut run) = run {
        *run = RunState::default();
    }
    if let Some(mut dash) = dash {
        *dash = DashState::default();
    }
    if let Some(mut attack) = attack {
        *attack = HammerAttackState::IDLE;
    }
}

#[derive(Clone)]
struct LifeSnapshot {
    entity: Entity,
    actor_id: u64,
    character: Option<CharacterId>,
    position: Option<Position>,
    facing: Option<BodyFacing>,
    life: CharacterLifeState,
    death_confirm: DeathConfirmIntent,
    status: Option<StatusEffectState>,
}

fn find_reviver(
    target_entity: Entity,
    target_character: Option<&CharacterId>,
    target_position: Option<&Position>,
    target_facing: Option<&BodyFacing>,
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
    actors: &[LifeSnapshot],
) -> Option<u64> {
    let mut candidates = actors
        .iter()
        .filter(|candidate| {
            candidate.entity != target_entity
                && candidate.life.is_alive()
                && candidate.death_confirm.pressed
                && !candidate
                    .status
                    .is_some_and(|status| status.blocks_action_buttons())
                && bodies_overlap(
                    candidate.entity,
                    target_character,
                    target_position,
                    target_facing,
                    hurt_geometry,
                    actors,
                )
        })
        .map(|candidate| candidate.actor_id)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.into_iter().next()
}

fn revival_continues(
    reviver_actor_id: Option<u64>,
    target_entity: Entity,
    target_character: Option<&CharacterId>,
    target_position: Option<&Position>,
    target_facing: Option<&BodyFacing>,
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
    actors: &[LifeSnapshot],
) -> bool {
    let Some(reviver_actor_id) = reviver_actor_id else {
        return false;
    };
    actors.iter().any(|candidate| {
        candidate.actor_id == reviver_actor_id
            && candidate.entity != target_entity
            && candidate.life.is_alive()
            && candidate.death_confirm.pressed
            && !candidate
                .status
                .is_some_and(|status| status.blocks_action_buttons())
            && bodies_overlap(
                candidate.entity,
                target_character,
                target_position,
                target_facing,
                hurt_geometry,
                actors,
            )
    })
}

fn bodies_overlap(
    reviver_entity: Entity,
    target_character: Option<&CharacterId>,
    target_position: Option<&Position>,
    target_facing: Option<&BodyFacing>,
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
    actors: &[LifeSnapshot],
) -> bool {
    let (Some(hurt_geometry), Some(target_character), Some(target_position), Some(target_facing)) = (
        hurt_geometry,
        target_character,
        target_position,
        target_facing,
    ) else {
        return false;
    };
    let Some(reviver) = actors.iter().find(|actor| actor.entity == reviver_entity) else {
        return false;
    };
    let (Some(reviver_character), Some(reviver_position), Some(reviver_facing)) =
        (&reviver.character, reviver.position, reviver.facing)
    else {
        return false;
    };
    let Some(reviver_hurt) = hurt_geometry.character(reviver_character) else {
        return false;
    };
    let Some(target_hurt) = hurt_geometry.character(target_character) else {
        return false;
    };
    let reviver_transform = hurt_transform(reviver_hurt, reviver_position, reviver_facing);
    let target_transform = posed_hurt_transform(
        target_hurt,
        *target_position,
        *target_facing,
        DEAD_BODY_SCALE,
        DEAD_BODY_ROTATION_RADIANS,
    );
    reviver_hurt.components.iter().any(|reviver_component| {
        target_hurt.components.iter().any(|target_component| {
            components_overlap(
                reviver_component,
                reviver_transform,
                target_component,
                target_transform,
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Update};
    use world01_configs::load_embedded;
    use world01_content::{CharacterHurtGeometryCatalog, RuntimeContent};
    use world01_design::{load_embedded as load_game_design, load_world01_embedded};
    use world01_world_data::{BodyFacing, CharacterId, MovementIntent, SelectedCharacter};

    fn test_app() -> App {
        let runtime = load_embedded().expect("embedded runtime parses");
        let design = load_world01_embedded().expect("embedded World 01 design parses");
        let mut app = App::new();
        app.insert_resource(
            CharacterLifeRules::from_design(runtime.simulation.ticks_per_second, &design.health)
                .expect("embedded character life design is valid"),
        )
        .insert_resource(
            CharacterHurtGeometryCatalog::from_content(
                &RuntimeContent::load_embedded().expect("embedded runtime content is valid"),
                &load_game_design().expect("embedded game design parses").hurt,
            )
            .expect("embedded hurt geometry is valid"),
        )
        .add_systems(Update, update_character_life);
        app
    }

    fn spawn_actor(app: &mut App, id: u64, health: f32, life: CharacterLifeState) -> Entity {
        app.world_mut()
            .spawn((
                ActorId(id),
                SelectedCharacter(CharacterId("hammerer".into())),
                Position::ZERO,
                BodyFacing::Authored,
                MovementIntent::ZERO,
                MovementVelocity::ZERO,
                MovementDirection::ZERO,
                CharacterHealth {
                    current: health,
                    maximum: 100.0,
                },
                life,
                DeathConfirmationState::default(),
                RevivalState::IDLE,
                RespawnState::default(),
                DeathConfirmIntent::RELEASED,
                StatusEffectState::default(),
            ))
            .id()
    }

    #[test]
    fn zero_health_enters_dead_and_stops_movement() {
        let mut app = test_app();
        let actor = spawn_actor(&mut app, 1, -10.0, CharacterLifeState::Alive);
        app.world_mut()
            .get_mut::<MovementVelocity>(actor)
            .unwrap()
            .x = 1.0;
        app.update();
        assert_eq!(
            app.world().get::<CharacterLifeState>(actor),
            Some(&CharacterLifeState::Dead)
        );
        assert_eq!(
            app.world().get::<CharacterHealth>(actor).unwrap().current,
            0.0
        );
        assert_eq!(
            app.world().get::<MovementVelocity>(actor),
            Some(&MovementVelocity::ZERO)
        );
    }

    #[test]
    fn releasing_confirmation_returns_to_dead_and_decays_one_tick() {
        let mut app = test_app();
        let actor = spawn_actor(&mut app, 1, 0.0, CharacterLifeState::DeathConfirming);
        app.world_mut()
            .get_mut::<DeathConfirmationState>(actor)
            .unwrap()
            .held_ticks = 60;
        app.update();
        assert_eq!(
            app.world().get::<CharacterLifeState>(actor),
            Some(&CharacterLifeState::Dead)
        );
        assert_eq!(
            app.world()
                .get::<DeathConfirmationState>(actor)
                .unwrap()
                .held_ticks,
            59
        );
    }

    #[test]
    fn confirming_for_four_seconds_respawns_with_forty_percent_health() {
        let mut app = test_app();
        let actor = spawn_actor(&mut app, 1, 0.0, CharacterLifeState::Dead);
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(actor)
            .unwrap()
            .pressed = true;
        for _ in 0..240 {
            app.update();
        }
        assert_eq!(
            app.world().get::<CharacterLifeState>(actor),
            Some(&CharacterLifeState::Alive)
        );
        assert!(
            (app.world().get::<CharacterHealth>(actor).unwrap().current - 40.0).abs()
                < f32::EPSILON
        );
        assert_eq!(app.world().get::<RespawnState>(actor).unwrap().count, 1);
    }

    #[test]
    fn confirmation_angle_matches_the_configured_velocity_ramp() {
        let runtime = load_embedded().expect("embedded runtime parses");
        let design = load_world01_embedded().expect("embedded World 01 design parses");
        let rules =
            CharacterLifeRules::from_design(runtime.simulation.ticks_per_second, &design.health)
                .expect("valid life rules");
        assert!((rules.confirmation_angle_radians(240.0).to_degrees() - 3168.0).abs() < 0.01);
        assert!((rules.confirmation_progress(120.0) - 0.5).abs() < 0.0001);
        assert_eq!(rules.revival_duration_ticks(), 480);
    }

    #[test]
    fn deterministic_respawns_stay_inside_the_ankh_radius() {
        let first = choose_respawn_position(
            7,
            1,
            Position::ZERO,
            None,
            None,
            4.0,
            &[(Ankh::new(0), Position::ZERO)],
            &[],
            None,
        );
        assert_eq!(first, Position::ZERO);
    }

    #[test]
    fn overlapping_alive_player_revives_after_eight_seconds() {
        let mut app = test_app();
        let target = spawn_actor(&mut app, 1, 0.0, CharacterLifeState::Dead);
        let reviver = spawn_actor(&mut app, 2, 100.0, CharacterLifeState::Alive);
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(reviver)
            .unwrap()
            .pressed = true;

        app.update();
        assert_eq!(
            app.world().get::<CharacterLifeState>(target),
            Some(&CharacterLifeState::Reviving)
        );
        assert_eq!(
            app.world()
                .get::<RevivalState>(target)
                .unwrap()
                .reviver_actor_id,
            Some(2)
        );

        for _ in 0..480 {
            app.update();
        }

        assert_eq!(
            app.world().get::<CharacterLifeState>(target),
            Some(&CharacterLifeState::Alive)
        );
        assert!(
            (app.world().get::<CharacterHealth>(target).unwrap().current - 80.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn target_confirmation_has_priority_over_an_active_revival() {
        let mut app = test_app();
        let target = spawn_actor(&mut app, 1, 0.0, CharacterLifeState::Reviving);
        let reviver = spawn_actor(&mut app, 2, 100.0, CharacterLifeState::Alive);
        *app.world_mut().get_mut::<RevivalState>(target).unwrap() = RevivalState {
            reviver_actor_id: Some(2),
            held_ticks: 420,
        };
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(target)
            .unwrap()
            .pressed = true;
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(reviver)
            .unwrap()
            .pressed = true;

        app.update();

        assert_eq!(
            app.world().get::<CharacterLifeState>(target),
            Some(&CharacterLifeState::DeathConfirming)
        );
        assert_eq!(
            app.world().get::<RevivalState>(target),
            Some(&RevivalState::IDLE)
        );
    }

    #[test]
    fn stunning_the_reviver_cancels_revival() {
        let mut app = test_app();
        let target = spawn_actor(&mut app, 1, 0.0, CharacterLifeState::Dead);
        let reviver = spawn_actor(&mut app, 2, 100.0, CharacterLifeState::Alive);
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(reviver)
            .unwrap()
            .pressed = true;
        app.update();
        app.world_mut()
            .get_mut::<StatusEffectState>(reviver)
            .unwrap()
            .stunned_ticks = 60;

        app.update();

        assert_eq!(
            app.world().get::<CharacterLifeState>(target),
            Some(&CharacterLifeState::Dead)
        );
        assert_eq!(
            app.world().get::<RevivalState>(target),
            Some(&RevivalState::IDLE)
        );
    }
}

use std::{error::Error, fmt};

use bevy::prelude::{Entity, MessageWriter, Query, Res, Resource};
use world01_design::LocomotionConfig;
use world01_world_data::{
    CharacterHealth, CharacterLifeState, CharacterMass, DashIntent, DashState, MovementIntent,
    MovementVelocity, RunIntent, RunState, StaminaState, StatusEffectState,
};

use crate::condition::ActorCondition;
#[cfg(test)]
use crate::damage::apply_damage;
use crate::damage::{DamageDealt, DamageSource};
use crate::movement::MovementStep;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ExertionRules {
    seconds_per_tick: f32,
    default_max_stamina: f32,
    stamina_regeneration_per_tick_ratio: f32,
    run_speed_multiplier: f32,
    run_drain_per_tick: f32,
    dash_cost_ratio: f32,
    dash_speed_multiplier: f32,
    dash_duration_seconds: f32,
    dash_invulnerability_seconds: f32,
    knockdown_duration_ticks: u32,
    knockdown_damage_ratio: f32,
}

impl ExertionRules {
    pub fn from_design(
        ticks_per_second: u32,
        design: &LocomotionConfig,
    ) -> Result<Self, ExertionConfigError> {
        if ticks_per_second == 0 || !design.is_valid() {
            return Err(ExertionConfigError);
        }

        let seconds_per_tick = 1.0 / ticks_per_second as f32;
        let locomotion = *design;
        Ok(Self {
            seconds_per_tick,
            default_max_stamina: locomotion.default_max_stamina,
            stamina_regeneration_per_tick_ratio: locomotion.stamina_regeneration_percent_per_second
                / 100.0
                * seconds_per_tick,
            run_speed_multiplier: locomotion.run_speed_multiplier,
            run_drain_per_tick: locomotion.run_drain_per_second * seconds_per_tick,
            dash_cost_ratio: locomotion.dash_cost_percent / 100.0,
            dash_speed_multiplier: locomotion.dash_speed_multiplier,
            dash_duration_seconds: locomotion.dash_duration_seconds,
            dash_invulnerability_seconds: locomotion.dash_invulnerability_seconds,
            knockdown_duration_ticks: (locomotion.knockdown_duration_seconds
                * ticks_per_second as f32)
                .round()
                .max(1.0) as u32,
            knockdown_damage_ratio: locomotion.knockdown_damage_percent_max_hp / 100.0,
        })
    }

    pub fn default_max_stamina(self) -> f32 {
        self.default_max_stamina
    }

    pub fn seconds_per_tick(self) -> f32 {
        self.seconds_per_tick
    }

    /// Stamina recovered in one tick, proportional to the actor's maximum.
    pub fn regeneration_per_tick(self, maximum_stamina: f32) -> f32 {
        maximum_stamina * self.stamina_regeneration_per_tick_ratio
    }

    /// Stamina spent in one tick of running.
    pub fn run_drain_per_tick(self) -> f32 {
        self.run_drain_per_tick
    }

    /// How much faster than its normal speed an actor moves while running.
    pub fn speed_multiplier(self, running: bool) -> f32 {
        if running {
            self.run_speed_multiplier
        } else {
            1.0
        }
    }

    /// Stamina a dash costs, proportional to the actor's maximum.
    pub fn dash_cost(self, maximum_stamina: f32) -> f32 {
        maximum_stamina * self.dash_cost_ratio
    }

    /// The velocity a dash locks in, taken from the actor's current motion.
    pub fn dash_velocity(self, normal: MovementVelocity) -> MovementVelocity {
        normal.scaled(self.dash_speed_multiplier)
    }

    /// Whether an actor in this state may begin a dash this tick.
    ///
    /// Reads no tuning value today; it is the place a stamina threshold or a
    /// cooldown would land, and the question a bot or a UI needs to ask.
    pub fn may_start_dash(
        self,
        pressed_edge: bool,
        dash: DashState,
        condition: ActorCondition,
        normal: MovementVelocity,
    ) -> bool {
        pressed_edge
            && !dash.active
            && !condition.blocks_action_buttons()
            && normal.length() > f32::EPSILON
    }

    /// Whether a dash that started this many seconds ago has run its course.
    pub fn dash_has_ended(self, elapsed_seconds: f32) -> bool {
        elapsed_seconds >= self.dash_duration_seconds
    }

    /// Whether a dash is inside its invulnerability window.
    pub fn dash_is_invulnerable(self, elapsed_seconds: f32) -> bool {
        let start = (self.dash_duration_seconds - self.dash_invulnerability_seconds) * 0.5;
        elapsed_seconds >= start && elapsed_seconds < start + self.dash_invulnerability_seconds
    }

    /// How long exhaustion keeps an actor down.
    pub fn knockdown_ticks(self) -> u32 {
        self.knockdown_duration_ticks
    }

    /// The damage exhaustion deals, proportional to the actor's maximum health.
    pub fn knockdown_damage(self, maximum_health: f32) -> f32 {
        maximum_health * self.knockdown_damage_ratio
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExertionConfigError;

impl fmt::Display for ExertionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("exertion configuration is invalid")
    }
}

impl Error for ExertionConfigError {}

pub fn update_exertion(
    rules: Res<ExertionRules>,
    movement_step: Res<MovementStep>,
    mut damage: MessageWriter<DamageDealt>,
    mut players: Query<(
        Entity,
        &MovementIntent,
        &CharacterMass,
        &RunIntent,
        &DashIntent,
        &mut MovementVelocity,
        &mut StaminaState,
        &mut RunState,
        &mut DashState,
        &mut StatusEffectState,
        &CharacterHealth,
        Option<&CharacterLifeState>,
    )>,
) {
    for (
        entity,
        movement_intent,
        mass,
        run_intent,
        dash_intent,
        mut velocity,
        mut stamina,
        mut run,
        mut dash,
        mut status,
        health,
        life,
    ) in &mut players
    {
        let condition = ActorCondition::new(Some(&*status), life);
        let mut stamina_delta = rules.regeneration_per_tick(stamina.maximum);

        let run_pressed_edge = run_intent.pressed && !run.input_pressed;
        let dash_pressed_edge = dash_intent.pressed && !dash.input_pressed;
        run.input_pressed = run_intent.pressed;
        dash.input_pressed = dash_intent.pressed;

        if run_pressed_edge && !condition.blocks_action_buttons() {
            run.toggled = !run.toggled;
        }

        if condition.blocks_all_input() {
            run.active = false;
            run.toggled = false;
        }
        // A dash is movement, so anything that blocks movement ends it - which
        // includes ROOTED, where the actor keeps its input but loses its feet.
        if condition.blocks_movement() {
            dash.active = false;
            dash.invulnerable = false;
            dash.velocity = MovementVelocity::ZERO;
        }

        let normal_velocity = if condition.blocks_movement() {
            MovementVelocity::ZERO
        } else {
            movement_step.velocity(
                *movement_intent,
                mass.normal_speed_meters_per_second,
                rules.speed_multiplier(run.toggled),
            )
        };

        let mut started_dash = false;
        if rules.may_start_dash(dash_pressed_edge, *dash, condition, normal_velocity) {
            stamina_delta -= rules.dash_cost(stamina.maximum);
            dash.active = true;
            dash.elapsed_seconds = 0.0;
            dash.velocity = rules.dash_velocity(normal_velocity);
            started_dash = true;
        }

        run.active = run.toggled
            && !condition.blocks_all_input()
            && (normal_velocity.length() > f32::EPSILON || dash.active);

        if run.active {
            stamina_delta -= rules.run_drain_per_tick();
        }

        let mut current_velocity = if dash.active {
            dash.velocity
        } else {
            normal_velocity
        };

        stamina.current = (stamina.current + stamina_delta).clamp(0.0, stamina.maximum.max(0.0));
        if stamina.current <= 0.0 && (run.active || started_dash) {
            stamina.current = 0.0;
            run.toggled = false;
            run.active = false;
            dash.active = false;
            dash.invulnerable = false;
            dash.velocity = MovementVelocity::ZERO;
            current_velocity = MovementVelocity::ZERO;
            status.knockdowned_ticks = rules.knockdown_ticks();
            damage.write(DamageDealt {
                target: entity,
                source: DamageSource::Exhaustion,
                amount: rules.knockdown_damage(health.maximum),
            });
        }

        *velocity = current_velocity;
    }
}

/// Advances a running dash after the movement it produced has been applied.
///
/// The tick a dash ends still moves at dash speed - the velocity was already
/// integrated - and only then falls back to the actor's normal velocity, so
/// facing follows the current input again immediately. That ordering is why
/// this runs after `integrate_movement` rather than inside `update_exertion`.
pub fn advance_dash(
    rules: Res<ExertionRules>,
    movement_step: Res<MovementStep>,
    mut actors: Query<(
        &MovementIntent,
        &CharacterMass,
        &RunState,
        &mut DashState,
        &mut MovementVelocity,
        Option<&StatusEffectState>,
        Option<&CharacterLifeState>,
    )>,
) {
    for (intent, mass, run, mut dash, mut velocity, status, life) in &mut actors {
        if !dash.active {
            continue;
        }
        dash.elapsed_seconds += rules.seconds_per_tick();
        dash.invulnerable = rules.dash_is_invulnerable(dash.elapsed_seconds);
        if !rules.dash_has_ended(dash.elapsed_seconds) {
            continue;
        }
        dash.active = false;
        dash.invulnerable = false;
        dash.velocity = MovementVelocity::ZERO;
        *velocity = if ActorCondition::new(status, life).blocks_movement() {
            MovementVelocity::ZERO
        } else {
            movement_step.velocity(
                *intent,
                mass.normal_speed_meters_per_second,
                rules.speed_multiplier(run.toggled),
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Entity, IntoScheduleConfigs, Update};
    use world01_configs::load_embedded;
    use world01_design::load_world01_embedded;
    use world01_world_data::Position;

    fn test_app() -> App {
        let runtime = load_embedded().expect("embedded runtime configuration parses");
        let design = load_world01_embedded().expect("embedded World 01 design parses");
        let mut app = App::new();
        app.insert_resource(
            MovementStep::from_runtime(&runtime).expect("embedded runtime configuration is valid"),
        )
        .insert_resource(
            ExertionRules::from_design(runtime.simulation.ticks_per_second, &design.locomotion)
                .expect("embedded exertion configuration is valid"),
        )
        .add_message::<DamageDealt>()
        .add_systems(
            Update,
            (
                update_exertion,
                crate::movement::integrate_movement,
                advance_dash,
                apply_damage,
            )
                .chain(),
        );
        app
    }

    fn spawn_player(app: &mut App, maximum_stamina: f32) -> Entity {
        app.world_mut()
            .spawn((
                MovementIntent::ZERO,
                CharacterMass::new(1.0, 0.0, 1.0, 0.8),
                RunIntent::RELEASED,
                DashIntent::RELEASED,
                Position::ZERO,
                MovementVelocity::ZERO,
                StaminaState::full(maximum_stamina),
                RunState::default(),
                DashState::default(),
                StatusEffectState::default(),
                CharacterHealth::full(100.0),
            ))
            .id()
    }

    #[test]
    fn run_toggle_is_potential_only_while_stationary() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 100.0);
        app.world_mut()
            .get_mut::<RunIntent>(player)
            .expect("test player has run input")
            .pressed = true;

        app.update();

        let run = app.world().get::<RunState>(player).unwrap();
        let stamina = app.world().get::<StaminaState>(player).unwrap();
        assert!(run.toggled);
        assert!(!run.active);
        assert!((stamina.current - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn dead_player_cannot_move_from_a_nonzero_intent() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 100.0);
        app.world_mut()
            .entity_mut(player)
            .insert(CharacterLifeState::Dead);
        app.world_mut().get_mut::<MovementIntent>(player).unwrap().x = 1.0;

        app.update();

        assert_eq!(app.world().get::<Position>(player), Some(&Position::ZERO));
        assert_eq!(
            app.world().get::<MovementVelocity>(player),
            Some(&MovementVelocity::ZERO)
        );
    }

    #[test]
    fn run_uses_absolute_drain_and_maximum_scaled_regeneration() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 200.0);
        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("test player has movement input") = MovementIntent::new(1.0, 0.0);
        *app.world_mut()
            .get_mut::<RunIntent>(player)
            .expect("test player has run input") = RunIntent::PRESSED;

        app.update();

        let velocity = app.world().get::<MovementVelocity>(player).unwrap();
        let stamina = app.world().get::<StaminaState>(player).unwrap();
        assert!((velocity.x - 1.2).abs() < f32::EPSILON);
        assert!((stamina.current - (200.0 - 3.0 / 60.0)).abs() < 0.00001);
    }

    #[test]
    fn dash_cost_scales_with_maximum_stamina_and_locks_velocity() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 200.0);
        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("test player has movement input") = MovementIntent::new(1.0, 0.0);
        *app.world_mut()
            .get_mut::<DashIntent>(player)
            .expect("test player has dash input") = DashIntent::PRESSED;

        app.update();

        let velocity = app.world().get::<MovementVelocity>(player).unwrap();
        let stamina = app.world().get::<StaminaState>(player).unwrap();
        assert!((velocity.x - 1.6).abs() < f32::EPSILON);
        assert!((stamina.current - (200.0 - 34.0 + 5.0 / 60.0)).abs() < 0.00001);

        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("test player has movement input") = MovementIntent::new(0.0, 1.0);
        app.update();
        assert!(app.world().get::<DashState>(player).unwrap().active);
        assert!(
            (app.world().get::<MovementVelocity>(player).unwrap().x - 1.6).abs() < f32::EPSILON
        );
        for _ in 0..60 {
            app.update();
        }
        assert!(!app.world().get::<DashState>(player).unwrap().active);
        assert!(
            (app.world().get::<MovementVelocity>(player).unwrap().y - 0.8).abs() < f32::EPSILON
        );
    }

    #[test]
    fn depletion_applies_knockdown_damage_and_interrupts_dash() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 10.0);
        app.world_mut()
            .get_mut::<StaminaState>(player)
            .expect("test player has stamina")
            .current = 0.01;
        *app.world_mut()
            .get_mut::<MovementIntent>(player)
            .expect("test player has movement input") = MovementIntent::new(1.0, 0.0);
        *app.world_mut()
            .get_mut::<RunIntent>(player)
            .expect("test player has run input") = RunIntent::PRESSED;

        app.update();

        let run = app.world().get::<RunState>(player).unwrap();
        let dash = app.world().get::<DashState>(player).unwrap();
        let status = app.world().get::<StatusEffectState>(player).unwrap();
        let health = app.world().get::<CharacterHealth>(player).unwrap();
        assert!(!run.toggled);
        assert!(!dash.active);
        assert_eq!(status.knockdowned_ticks, 120);
        assert!((health.current - 95.0).abs() < f32::EPSILON);
        assert_eq!(
            app.world()
                .get::<MovementVelocity>(player)
                .unwrap()
                .length(),
            0.0
        );
    }

    #[test]
    fn a_root_ends_a_dash_but_leaves_the_run_toggle_alone() {
        let mut app = test_app();
        let actor = spawn_player(&mut app, 100.0);
        app.world_mut().entity_mut(actor).insert((
            MovementIntent::new(1.0, 0.0),
            DashState {
                active: true,
                elapsed_seconds: 0.1,
                velocity: MovementVelocity::new(1.6, 0.0),
                invulnerable: true,
                input_pressed: false,
            },
            RunState {
                toggled: true,
                active: true,
                input_pressed: false,
            },
            StatusEffectState {
                rooted_ticks: 30,
                ..StatusEffectState::default()
            },
        ));

        app.update();

        let dash = app.world().get::<DashState>(actor).unwrap();
        assert!(!dash.active, "a root ends the dash");
        assert!(!dash.invulnerable, "and with it the invulnerability window");
        assert_eq!(dash.velocity, MovementVelocity::ZERO);
        assert!(
            app.world().get::<RunState>(actor).unwrap().toggled,
            "a root does not clear the run toggle, which only blocked input does"
        );
        assert_eq!(
            app.world().get::<Position>(actor),
            Some(&Position::ZERO),
            "and the rooted actor does not move"
        );
    }
}

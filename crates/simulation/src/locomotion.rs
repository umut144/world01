use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use world01_configs::DesignConfig;
use world01_world_data::{
    CharacterHealth, CharacterLifeState, CharacterMass, DashIntent, DashState, MovementIntent,
    MovementVelocity, Position, RunIntent, RunState, StaminaState, StatusEffectState,
};

use crate::movement::MovementStep;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LocomotionRules {
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

impl LocomotionRules {
    pub fn from_design(config: &DesignConfig) -> Result<Self, LocomotionConfigError> {
        if config.simulation.ticks_per_second == 0 || !config.locomotion.is_valid() {
            return Err(LocomotionConfigError);
        }

        let seconds_per_tick = 1.0 / config.simulation.ticks_per_second as f32;
        let locomotion = config.locomotion;
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
                * config.simulation.ticks_per_second as f32)
                .round()
                .max(1.0) as u32,
            knockdown_damage_ratio: locomotion.knockdown_damage_percent_max_hp / 100.0,
        })
    }

    pub fn default_max_stamina(self) -> f32 {
        self.default_max_stamina
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocomotionConfigError;

impl fmt::Display for LocomotionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("locomotion configuration is invalid")
    }
}

impl Error for LocomotionConfigError {}

pub fn update_locomotion(
    rules: Res<LocomotionRules>,
    movement_step: Res<MovementStep>,
    mut players: Query<(
        &MovementIntent,
        &CharacterMass,
        &RunIntent,
        &DashIntent,
        &mut Position,
        &mut MovementVelocity,
        &mut StaminaState,
        &mut RunState,
        &mut DashState,
        &mut StatusEffectState,
        &mut CharacterHealth,
        Option<&CharacterLifeState>,
    )>,
) {
    for (
        movement_intent,
        mass,
        run_intent,
        dash_intent,
        mut position,
        mut velocity,
        mut stamina,
        mut run,
        mut dash,
        mut status,
        mut health,
        life,
    ) in &mut players
    {
        status.tick();
        let mut stamina_delta = stamina.maximum * rules.stamina_regeneration_per_tick_ratio;

        let run_pressed_edge = run_intent.pressed && !run.input_pressed;
        let dash_pressed_edge = dash_intent.pressed && !dash.input_pressed;
        run.input_pressed = run_intent.pressed;
        dash.input_pressed = dash_intent.pressed;

        if run_pressed_edge && !status.blocks_action_buttons() {
            run.toggled = !run.toggled;
        }

        let life_blocks_input = life.is_some_and(|life| !life.is_alive());
        if status.blocks_all_input() || life_blocks_input {
            run.active = false;
            run.toggled = false;
            dash.active = false;
            dash.invulnerable = false;
            dash.velocity = MovementVelocity::ZERO;
        }

        let normal_velocity = if status.blocks_movement() || life_blocks_input {
            MovementVelocity::ZERO
        } else {
            let multiplier = if run.toggled {
                rules.run_speed_multiplier
            } else {
                1.0
            };
            movement_step.velocity(
                *movement_intent,
                mass.normal_speed_meters_per_second,
                multiplier,
            )
        };

        let mut started_dash = false;
        if dash_pressed_edge
            && !dash.active
            && !status.blocks_action_buttons()
            && !life_blocks_input
            && normal_velocity.length() > f32::EPSILON
        {
            stamina_delta -= stamina.maximum * rules.dash_cost_ratio;
            dash.active = true;
            dash.elapsed_seconds = 0.0;
            dash.velocity = normal_velocity.scaled(rules.dash_speed_multiplier);
            started_dash = true;
        }

        run.active = run.toggled
            && !status.blocks_all_input()
            && !life_blocks_input
            && (normal_velocity.length() > f32::EPSILON || dash.active);

        if run.active {
            stamina_delta -= rules.run_drain_per_tick;
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
            status.knockdowned_ticks = rules.knockdown_duration_ticks;
            health.current =
                (health.current - health.maximum * rules.knockdown_damage_ratio).max(0.0);
        }

        if current_velocity != MovementVelocity::ZERO {
            let current = Vec2::new(position.x, position.y);
            let displacement =
                Vec2::new(current_velocity.x, current_velocity.y) * rules.seconds_per_tick;
            let proposed = current + displacement;
            *position = Position::new(proposed.x, proposed.y);
        }

        if dash.active {
            dash.elapsed_seconds += rules.seconds_per_tick;
            dash.invulnerable = dash.elapsed_seconds >= dash_invulnerability_start(*rules)
                && dash.elapsed_seconds < dash_invulnerability_end(*rules);
            if dash.elapsed_seconds >= rules.dash_duration_seconds {
                dash.active = false;
                dash.invulnerable = false;
                dash.velocity = MovementVelocity::ZERO;
                current_velocity = normal_velocity;
            }
        }

        *velocity = current_velocity;
    }
}

fn dash_invulnerability_start(rules: LocomotionRules) -> f32 {
    (rules.dash_duration_seconds - rules.dash_invulnerability_seconds) * 0.5
}

fn dash_invulnerability_end(rules: LocomotionRules) -> f32 {
    dash_invulnerability_start(rules) + rules.dash_invulnerability_seconds
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Entity, Update};
    use world01_configs::load_embedded;

    fn test_app() -> App {
        let design = load_embedded().expect("embedded design configuration parses");
        let mut app = App::new();
        app.insert_resource(
            MovementStep::from_design(&design).expect("embedded movement configuration is valid"),
        )
        .insert_resource(
            LocomotionRules::from_design(&design)
                .expect("embedded locomotion configuration is valid"),
        )
        .add_systems(Update, update_locomotion);
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
}

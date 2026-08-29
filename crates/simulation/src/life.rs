use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource};
use game01_configs::DesignConfig;
use game01_world_data::{
    CharacterHealth, CharacterLifeState, DeathConfirmIntent, DeathConfirmationState,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CharacterLifeRules {
    seconds_per_tick: f32,
    confirmation_duration_ticks: u32,
    confirmation_initial_radians_per_second: f32,
    confirmation_angular_acceleration: f32,
}

impl CharacterLifeRules {
    pub fn from_design(config: &DesignConfig) -> Result<Self, CharacterLifeConfigError> {
        if config.simulation.ticks_per_second == 0 || !config.health.is_valid() {
            return Err(CharacterLifeConfigError);
        }

        let seconds_per_tick = 1.0 / config.simulation.ticks_per_second as f32;
        let duration = config.health.downed_confirmation_seconds;
        let initial = config
            .health
            .downed_confirmation_initial_degrees_per_second
            .to_radians();
        let maximum = config
            .health
            .downed_confirmation_max_degrees_per_second
            .to_radians();
        Ok(Self {
            seconds_per_tick,
            confirmation_duration_ticks: (duration * config.simulation.ticks_per_second as f32)
                .round()
                .max(1.0) as u32,
            confirmation_initial_radians_per_second: initial,
            confirmation_angular_acceleration: (maximum - initial) / duration,
        })
    }

    pub fn confirmation_duration_ticks(self) -> u32 {
        self.confirmation_duration_ticks
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
    mut players: Query<(
        &mut CharacterHealth,
        &mut CharacterLifeState,
        &mut DeathConfirmationState,
        &DeathConfirmIntent,
    )>,
) {
    for (mut health, mut life, mut confirmation, death_confirm) in &mut players {
        let maximum = health.maximum.max(0.0);
        health.maximum = maximum;
        health.current = health.current.clamp(0.0, maximum);

        match *life {
            CharacterLifeState::Alive if health.current <= 0.0 => {
                *life = CharacterLifeState::Downed;
                confirmation.held_ticks = 0;
            }
            CharacterLifeState::Alive => {
                confirmation.held_ticks = 0;
            }
            CharacterLifeState::Downed => {
                health.current = 0.0;
                if death_confirm.pressed {
                    confirmation.held_ticks = confirmation
                        .held_ticks
                        .saturating_add(1)
                        .min(rules.confirmation_duration_ticks());
                    if confirmation.held_ticks >= rules.confirmation_duration_ticks() {
                        *life = CharacterLifeState::Dead;
                        confirmation.held_ticks = 0;
                    }
                } else {
                    confirmation.held_ticks = confirmation.held_ticks.saturating_sub(1);
                }
            }
            CharacterLifeState::Dead => {
                health.current = 0.0;
                confirmation.held_ticks = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Entity, Update};
    use game01_configs::load_embedded;

    fn test_app() -> App {
        let config = load_embedded().expect("embedded config parses");
        let mut app = App::new();
        app.insert_resource(
            CharacterLifeRules::from_design(&config)
                .expect("embedded character life design is valid"),
        )
        .add_systems(Update, update_character_life);
        app
    }

    fn spawn_player(app: &mut App, health: f32, life: CharacterLifeState) -> Entity {
        app.world_mut()
            .spawn((
                CharacterHealth {
                    current: health,
                    maximum: 100.0,
                },
                life,
                DeathConfirmationState::default(),
                DeathConfirmIntent::RELEASED,
            ))
            .id()
    }

    #[test]
    fn zero_health_enters_downed_and_clamps_health() {
        let mut app = test_app();
        let player = spawn_player(&mut app, -10.0, CharacterLifeState::Alive);

        app.update();

        assert_eq!(
            app.world().get::<CharacterLifeState>(player),
            Some(&CharacterLifeState::Downed)
        );
        assert_eq!(
            app.world().get::<CharacterHealth>(player).unwrap().current,
            0.0
        );
    }

    #[test]
    fn releasing_confirmation_decays_one_tick_without_resetting() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 0.0, CharacterLifeState::Downed);
        app.world_mut()
            .get_mut::<DeathConfirmationState>(player)
            .unwrap()
            .held_ticks = 60;

        app.update();

        assert_eq!(
            app.world()
                .get::<DeathConfirmationState>(player)
                .unwrap()
                .held_ticks,
            59
        );
    }

    #[test]
    fn holding_confirmation_for_four_seconds_enters_dead() {
        let mut app = test_app();
        let player = spawn_player(&mut app, 0.0, CharacterLifeState::Downed);
        app.world_mut()
            .get_mut::<DeathConfirmIntent>(player)
            .unwrap()
            .pressed = true;

        for _ in 0..240 {
            app.update();
        }

        assert_eq!(
            app.world().get::<CharacterLifeState>(player),
            Some(&CharacterLifeState::Dead)
        );
    }

    #[test]
    fn confirmation_angle_matches_the_configured_velocity_ramp() {
        let config = load_embedded().expect("embedded config parses");
        let rules = CharacterLifeRules::from_design(&config).expect("valid life rules");
        let angle = rules.confirmation_angle_radians(240.0).to_degrees();

        assert!((angle - 3168.0).abs() < 0.01);
        assert!((rules.confirmation_progress(120.0) - 0.5).abs() < 0.0001);
    }
}

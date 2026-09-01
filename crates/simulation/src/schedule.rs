use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    advance_dash, advance_hammer_attacks, advance_mage_attacks, apply_damage,
    apply_hammer_strike_damage, apply_mage_beam_damage, constrain_embedded_hammer_reach,
    damage::DamageDealt, expire_mage_beams, finish_mage_cooldowns, integrate_movement,
    tick_status_effects, update_character_life, update_character_orientation, update_exertion,
    update_gaze_direction, update_weapon_aim,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationSet {
    /// Advances intent-driven character state for the current tick.
    GameplayStep,
    /// Sits between deciding a velocity and applying it to a position.
    ///
    /// Reserved for world and actor collision; no system runs here yet.
    Collision,
    /// Resolves the consequences of the gameplay step: damage, expiry, and life state.
    Resolution,
}

/// Selects which parts of the simulation step an app is allowed to run.
///
/// Both authorities run the identical gameplay step. Only the server resolves
/// damage, so a predicting client cannot invent hits that the server never saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationAuthority {
    /// The authoritative simulation: resolves damage as well.
    Server,
    /// A predicting client: replays the gameplay step without resolving damage.
    Predicted,
}

impl SimulationAuthority {
    const fn resolves_damage(self) -> bool {
        matches!(self, Self::Server)
    }
}

/// Adds the canonical deterministic simulation step to the schedule selected by the app.
///
/// This is the single definition of tick order for both the server and the
/// predicting client; neither app may register gameplay systems of its own.
pub fn add_simulation_step(
    app: &mut App,
    schedule: impl ScheduleLabel + Clone,
    authority: SimulationAuthority,
) {
    app.add_message::<DamageDealt>();
    app.add_systems(
        schedule.clone(),
        (
            update_gaze_direction,
            update_weapon_aim,
            advance_hammer_attacks,
            advance_mage_attacks,
            tick_status_effects,
            update_exertion,
            integrate_movement,
            advance_dash,
            constrain_embedded_hammer_reach,
            update_character_orientation,
        )
            .chain()
            .in_set(SimulationSet::GameplayStep),
    );
    if authority.resolves_damage() {
        app.add_systems(
            schedule.clone(),
            (apply_hammer_strike_damage, apply_mage_beam_damage)
                .chain()
                .before(apply_damage)
                .in_set(SimulationSet::Resolution),
        );
    }
    app.add_systems(
        schedule.clone(),
        (
            apply_damage,
            expire_mage_beams,
            finish_mage_cooldowns,
            update_character_life,
        )
            .chain()
            .in_set(SimulationSet::Resolution),
    );
    app.configure_sets(
        schedule.clone(),
        SimulationSet::Collision
            .after(update_exertion)
            .before(integrate_movement),
    );
    app.configure_sets(
        schedule,
        SimulationSet::Resolution.after(SimulationSet::GameplayStep),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_configs::load_embedded;
    use world01_content::{HammerCombatGeometry, RuntimeContent};
    use world01_design::{load_embedded as load_game_design, load_world01_embedded};
    use world01_world_data::{
        CharacterHealth, CharacterMass, DashIntent, DashState, MovementIntent, MovementVelocity,
        Position, RunIntent, RunState, StaminaState, StatusEffectState,
    };

    use crate::{
        CharacterLifeRules, ExertionRules, HammerAttackRules, MageAttackRules, MovementStep,
        WeaponAimRules,
    };

    /// What an actor looks like at the moment the collision phase runs.
    #[derive(Resource, Debug, Default)]
    struct CollisionProbe {
        velocity: MovementVelocity,
        position: Position,
        ran: bool,
    }

    fn record_collision_phase(
        mut probe: ResMut<CollisionProbe>,
        actors: Query<(&MovementVelocity, &Position)>,
    ) {
        for (velocity, position) in &actors {
            probe.velocity = *velocity;
            probe.position = *position;
            probe.ran = true;
        }
    }

    /// Pins the contract a collision system depends on: by the time the phase
    /// runs the velocity for this tick is decided, and the position it will
    /// produce has not been written yet.
    #[test]
    fn the_collision_phase_sees_a_decided_velocity_and_an_unmoved_position() {
        let config = load_embedded().expect("embedded runtime configuration parses");
        let world_design = load_world01_embedded().expect("embedded World 01 design parses");
        let game_design = load_game_design().expect("embedded game design parses");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let ticks = config.simulation.ticks_per_second;

        let mut app = App::new();
        app.init_resource::<CollisionProbe>()
            .insert_resource(MovementStep::from_runtime(&config).expect("runtime is valid"))
            .insert_resource(
                ExertionRules::from_design(ticks, &world_design.locomotion)
                    .expect("exertion design is valid"),
            )
            .insert_resource(
                WeaponAimRules::from_design(ticks, &world_design.weapon_aim)
                    .expect("weapon aim design is valid"),
            )
            .insert_resource(
                CharacterLifeRules::from_design(ticks, &world_design.health)
                    .expect("life design is valid"),
            )
            .insert_resource(
                HammerAttackRules::from_design(ticks, &game_design.hammer)
                    .expect("hammer design is valid"),
            )
            .insert_resource(
                HammerCombatGeometry::from_content(&content, &game_design.hammer.attack_components)
                    .expect("hammer geometry is valid"),
            )
            .insert_resource(
                MageAttackRules::from_design(ticks, &game_design.mage, &game_design.mage_eye_beams)
                    .expect("mage design is valid"),
            );
        add_simulation_step(&mut app, Update, SimulationAuthority::Predicted);
        app.add_systems(
            Update,
            record_collision_phase.in_set(SimulationSet::Collision),
        );

        let actor = app
            .world_mut()
            .spawn((
                MovementIntent::new(1.0, 0.0),
                CharacterMass::new(1.0, 0.0, 1.0, 0.6),
                RunIntent::RELEASED,
                DashIntent::RELEASED,
                MovementVelocity::ZERO,
                StaminaState::full(100.0),
                RunState::default(),
                DashState::default(),
                StatusEffectState::default(),
                CharacterHealth::full(100.0),
                Position::ZERO,
            ))
            .id();

        app.update();

        let probe = app.world().resource::<CollisionProbe>();
        assert!(probe.ran, "the collision phase runs inside the step");
        assert!(
            probe.velocity.x > 0.0,
            "the velocity is decided before the collision phase"
        );
        assert_eq!(
            probe.position,
            Position::ZERO,
            "the position is written only after the collision phase"
        );
        assert!(
            app.world()
                .get::<Position>(actor)
                .expect("the actor keeps its position")
                .x
                > 0.0,
            "the decided velocity still reaches the position"
        );
    }
}

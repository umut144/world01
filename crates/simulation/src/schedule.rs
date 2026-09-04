use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::{
    advance_dash, advance_hammer_attacks, advance_mage_attacks, apply_damage,
    apply_hammer_strike_damage, apply_mage_beam_damage, block_colliding_movement,
    constrain_embedded_hammer_reach, damage::DamageDealt, expire_mage_beams, finish_mage_cooldowns,
    integrate_movement, separate_characters_from_world, separate_overlapping_characters,
    tick_status_effects, update_character_life, update_character_orientation, update_exertion,
    update_gaze_direction, update_weapon_aim,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimulationSet {
    /// Advances intent-driven character state for the current tick.
    GameplayStep,
    /// Sits between deciding a velocity and applying it to a position.
    ///
    /// Where the server separates existing Character/Character and
    /// Character/world overlap, and movement that would end inside geometry is
    /// refused.
    Collision,
    /// Resolves the consequences of the gameplay step: damage, expiry, and life state.
    Resolution,
}

/// Selects which parts of the simulation step an app is allowed to run.
///
/// Both authorities run the identical intent-driven gameplay step. Only the
/// server resolves damage and existing overlap, so a predicting
/// client cannot invent authoritative outcomes the server never saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationAuthority {
    /// The authoritative simulation: resolves damage and existing overlap.
    Server,
    /// A predicting client: replays the gameplay step without resolving damage.
    Predicted,
}

impl SimulationAuthority {
    const fn resolves_damage(self) -> bool {
        matches!(self, Self::Server)
    }

    const fn separates_overlaps(self) -> bool {
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
    add_collision_systems(app, schedule.clone(), authority);
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

fn add_collision_systems(
    app: &mut App,
    schedule: impl ScheduleLabel + Clone,
    authority: SimulationAuthority,
) {
    if authority.separates_overlaps() {
        app.add_systems(
            schedule,
            (
                separate_overlapping_characters,
                separate_characters_from_world,
                block_colliding_movement,
            )
                .chain()
                .in_set(SimulationSet::Collision),
        );
    } else {
        app.add_systems(
            schedule,
            block_colliding_movement.in_set(SimulationSet::Collision),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_configs::load_embedded;
    use world01_content::{
        AuthoredFacing, CharacterCollisionGeometry, CharacterCollisionGeometryCatalog,
        CollisionComponentGeometry, HammerCombatGeometry, PlacedCollisionGeometry,
        RuntimeComponentGeometry, RuntimeContent, WorldCollisionGeometryCatalog,
    };
    use world01_design::{load_embedded as load_game_design, load_world01_embedded};
    use world01_world_data::{
        ActorId, AnkhLayout, BodyFacing, CharacterHealth, CharacterId, CharacterMass, DashIntent,
        DashState, MovementIntent, MovementVelocity, Position, RunIntent, RunState,
        SelectedCharacter, StaminaState, StatusEffectState,
    };

    use crate::{
        CharacterLifeRules, ExertionRules, HammerAttackRules, MageAttackRules, MovementStep,
        WeaponAimRules, WorldColliderGrid, WorldSeparationStep,
    };

    /// What an actor looks like at the moment the collision phase runs.
    #[derive(Resource, Debug, Default)]
    struct CollisionProbe {
        velocity: MovementVelocity,
        position: Position,
        ran: bool,
    }

    #[derive(Resource, Debug, Default)]
    struct SeparationProbe {
        positions: Vec<Position>,
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

    fn record_positions_after_separation(
        mut probe: ResMut<SeparationProbe>,
        actors: Query<&Position, With<ActorId>>,
    ) {
        probe.positions = actors.iter().copied().collect();
        probe
            .positions
            .sort_by(|first, second| first.x.total_cmp(&second.x));
    }

    fn collision_catalog() -> CharacterCollisionGeometryCatalog {
        let component = CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: "body".into(),
            name: "body".into(),
            vertices: vec![
                Vec2::splat(-0.2),
                Vec2::new(0.2, -0.2),
                Vec2::splat(0.2),
                Vec2::new(-0.2, 0.2),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the test square has valid collision topology");
        CharacterCollisionGeometryCatalog::from_geometries([(
            CharacterId("walker".into()),
            CharacterCollisionGeometry {
                authored_facing: AuthoredFacing::Right,
                components: vec![component],
            },
        )])
    }

    fn collision_only_app(authority: SimulationAuthority) -> App {
        collision_only_app_with_world(authority, WorldCollisionGeometryCatalog::default())
    }

    fn collision_only_app_with_world(
        authority: SimulationAuthority,
        world: WorldCollisionGeometryCatalog,
    ) -> App {
        let config = load_embedded().expect("embedded runtime configuration parses");
        let grid = WorldColliderGrid::from_catalog(&world);
        let mut app = App::new();
        app.insert_resource(collision_catalog())
            .insert_resource(world)
            .insert_resource(grid)
            .insert_resource(
                WorldSeparationStep::from_runtime(&config)
                    .expect("embedded world separation configuration is valid"),
            )
            .insert_resource(MovementStep::from_runtime(&config).expect("runtime is valid"));
        add_collision_systems(&mut app, Update, authority);
        app
    }

    fn overlapping_world() -> WorldCollisionGeometryCatalog {
        let component = CollisionComponentGeometry::from_geometry(RuntimeComponentGeometry {
            component_id: "world".into(),
            name: "world".into(),
            vertices: vec![
                Vec2::splat(-0.2),
                Vec2::new(0.2, -0.2),
                Vec2::splat(0.2),
                Vec2::new(-0.2, 0.2),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        })
        .expect("the world test square has valid collision topology");
        WorldCollisionGeometryCatalog {
            regions: vec![PlacedCollisionGeometry {
                instance_id: "world".into(),
                position: Position::new(0.3, 0.0),
                component,
            }],
        }
    }

    fn spawn_overlapping_pair(app: &mut App) -> (Entity, Entity) {
        let spawn = |app: &mut App, actor_id, x| {
            app.world_mut()
                .spawn((
                    ActorId(actor_id),
                    SelectedCharacter(CharacterId("walker".into())),
                    BodyFacing::Right,
                    CharacterMass::new(1.0, 0.0, 1.0, 0.6),
                    Position::new(x, 0.0),
                    MovementVelocity::ZERO,
                ))
                .id()
        };
        (spawn(app, 1, 0.0), spawn(app, 2, 0.3))
    }

    /// Pins the predicted-client contract: by the time its collision phase
    /// runs the velocity for this tick is decided, and requested movement has
    /// not yet been written to Position. Server authority may already have
    /// corrected pre-existing overlap at this point.
    #[test]
    fn the_predicted_collision_phase_sees_velocity_before_requested_movement() {
        let config = load_embedded().expect("embedded runtime configuration parses");
        let world_design = load_world01_embedded().expect("embedded World 01 design parses");
        let game_design = load_game_design().expect("embedded game design parses");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let ticks = config.simulation.ticks_per_second;

        let mut app = App::new();
        app.init_resource::<CollisionProbe>()
            .init_resource::<CharacterCollisionGeometryCatalog>()
            .init_resource::<WorldCollisionGeometryCatalog>()
            .init_resource::<WorldColliderGrid>()
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
            )
            .insert_resource(AnkhLayout { positions: vec![] });
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

    #[test]
    fn existing_overlap_is_separated_only_by_server_authority() {
        let mut predicted = collision_only_app(SimulationAuthority::Predicted);
        let (predicted_first, predicted_second) = spawn_overlapping_pair(&mut predicted);
        predicted.update();

        assert_eq!(
            predicted.world().get::<Position>(predicted_first),
            Some(&Position::ZERO)
        );
        assert_eq!(
            predicted.world().get::<Position>(predicted_second),
            Some(&Position::new(0.3, 0.0))
        );

        let mut server = collision_only_app(SimulationAuthority::Server);
        let (server_first, server_second) = spawn_overlapping_pair(&mut server);
        server.update();

        assert!(
            server
                .world()
                .get::<Position>(server_first)
                .is_some_and(|position| position.x < 0.0)
        );
        assert!(
            server
                .world()
                .get::<Position>(server_second)
                .is_some_and(|position| position.x > 0.3)
        );
    }

    #[test]
    fn existing_world_overlap_is_separated_only_by_server_authority() {
        let spawn = |app: &mut App| {
            app.world_mut()
                .spawn((
                    SelectedCharacter(CharacterId("walker".into())),
                    BodyFacing::Right,
                    Position::ZERO,
                    MovementVelocity::ZERO,
                ))
                .id()
        };

        let mut predicted =
            collision_only_app_with_world(SimulationAuthority::Predicted, overlapping_world());
        let predicted_actor = spawn(&mut predicted);
        predicted.update();
        assert_eq!(
            predicted.world().get::<Position>(predicted_actor),
            Some(&Position::ZERO)
        );

        let mut server =
            collision_only_app_with_world(SimulationAuthority::Server, overlapping_world());
        let server_actor = spawn(&mut server);
        server.update();
        assert_ne!(
            server.world().get::<Position>(server_actor),
            Some(&Position::ZERO)
        );
    }

    #[test]
    fn movement_blocking_sees_server_corrected_positions() {
        let mut app = collision_only_app(SimulationAuthority::Server);
        app.init_resource::<SeparationProbe>().add_systems(
            Update,
            record_positions_after_separation
                .after(separate_overlapping_characters)
                .after(separate_characters_from_world)
                .before(block_colliding_movement)
                .in_set(SimulationSet::Collision),
        );
        spawn_overlapping_pair(&mut app);

        app.update();

        let positions = &app.world().resource::<SeparationProbe>().positions;
        assert_eq!(positions.len(), 2);
        assert!(positions[0].x < 0.0);
        assert!(positions[1].x > 0.3);
    }
}

use std::collections::HashSet;

use bevy::{log::warn, prelude::*};
use world01_content::{CharacterHealthCatalog, RuntimeContent};
use world01_network::{
    MAX_CLIENTS, ServerJoinRequest, ServerNetworkSet, ServerWorldTemplateDebugRequest,
    WorldTemplateDebugPreset, configure_replicated_player, configure_replicated_world_state,
};
use world01_simulation::{
    CharacterAbilityCatalog, CharacterMassCatalog, ExertionRules, SimulationSet, WorldRuntimeSet,
    WorldRuntimeState,
};
use world01_world_data::{
    ActorId, AnchorOccupancy, AttackIntent, BodyFacing, CharacterHealth, CharacterLifeState,
    DashIntent, DashState, DeathConfirmIntent, DeathConfirmationState, GazeDirection, GazeIntent,
    MovementDirection, MovementIntent, MovementVelocity, PlacementRanks, PlayerOwner, Position,
    RespawnState, RevivalState, RunIntent, RunState, SelectedCharacter, StaminaState,
    StatusEffectState, WorldComposition, WorldOccupancyRequest, WorldTemplateCatalog,
};

const TEST_TEMPLATE_SCENE_ID: &str = "test_template02";
const FIRST_TEST_ANCHOR_ID: &str = "template_anchor_001";
const SECOND_TEST_ANCHOR_ID: &str = "template_anchor_002";

#[derive(Resource, Debug)]
struct NextActorId(u64);

impl Default for NextActorId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Resource, Debug, Default)]
struct PendingWorldTemplateDebugPreset {
    latest: Option<(u64, WorldTemplateDebugPreset)>,
}

impl PendingWorldTemplateDebugPreset {
    fn submit(&mut self, received_order: u64, preset: WorldTemplateDebugPreset) {
        if self
            .latest
            .is_none_or(|(current_order, _)| received_order > current_order)
        {
            self.latest = Some((received_order, preset));
        }
    }

    fn take(&mut self) -> Option<WorldTemplateDebugPreset> {
        self.latest.take().map(|(_, preset)| preset)
    }
}

pub struct ServerSessionPlugin;

#[derive(Component)]
struct AuthoritativeWorldState;

impl Plugin for ServerSessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextActorId>()
            .init_resource::<PendingWorldTemplateDebugPreset>()
            .add_systems(Startup, spawn_world_state)
            .add_systems(
                Update,
                (accept_join_requests, accept_world_template_debug_requests)
                    .after(ServerNetworkSet::ReceiveRequests),
            )
            .add_systems(
                FixedUpdate,
                (
                    submit_pending_world_template_debug_preset.before(WorldRuntimeSet::Rebuild),
                    publish_world_occupancy
                        .after(WorldRuntimeSet::Rebuild)
                        .before(SimulationSet::Collision),
                ),
            );
    }
}

fn accept_world_template_debug_requests(
    requests: Query<(Entity, &ServerWorldTemplateDebugRequest)>,
    players: Query<&PlayerOwner>,
    mut pending: ResMut<PendingWorldTemplateDebugPreset>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        commands.entity(entity).despawn();
        if !players.iter().any(|owner| owner.0 == request.owner()) {
            warn!(
                owner = request.owner(),
                "ignoring world Template debug request from a client without a joined Character"
            );
            continue;
        }
        pending.submit(request.received_order(), request.preset);
    }
}

fn submit_pending_world_template_debug_preset(
    mut requested_preset: ResMut<PendingWorldTemplateDebugPreset>,
    composition: Res<WorldComposition>,
    templates: Res<WorldTemplateCatalog>,
    ranks: Res<PlacementRanks>,
    mut pending_occupancy: ResMut<WorldOccupancyRequest>,
) {
    let Some(preset) = requested_preset.take() else {
        return;
    };
    let mut candidate = composition.clone();
    if let Err(error) =
        apply_world_template_debug_preset(&mut candidate, preset, &templates, &ranks)
    {
        warn!(%error, "ignoring invalid world Template debug request");
        return;
    }
    let offered_generation = pending_occupancy.latest().map_or(
        composition.occupancy().generation(),
        AnchorOccupancy::generation,
    );
    if let Err(error) = candidate.ensure_generation_newer_than(offered_generation) {
        warn!(%error, "cannot advance world Template debug request generation");
        return;
    }
    if !pending_occupancy.submit(candidate.occupancy().clone()) {
        warn!(
            generation = candidate.occupancy().generation(),
            "ignoring stale world Template debug request"
        );
    }
}

fn apply_world_template_debug_preset(
    composition: &mut WorldComposition,
    preset: WorldTemplateDebugPreset,
    templates: &WorldTemplateCatalog,
    ranks: &PlacementRanks,
) -> Result<bool, world01_world_data::WorldMapError> {
    let (first_occupied, second_occupied) = match preset {
        WorldTemplateDebugPreset::Empty => (false, false),
        WorldTemplateDebugPreset::FirstAnchor => (true, false),
        WorldTemplateDebugPreset::SecondAnchor => (false, true),
        WorldTemplateDebugPreset::BothAnchors => (true, true),
    };
    let mut changed = false;
    for (anchor_id, occupied) in [
        (FIRST_TEST_ANCHOR_ID, first_occupied),
        (SECOND_TEST_ANCHOR_ID, second_occupied),
    ] {
        changed |= if occupied {
            composition.set_occupant(anchor_id, TEST_TEMPLATE_SCENE_ID, templates, ranks)?
        } else {
            composition.clear_occupant(anchor_id, templates, ranks)?
        };
    }
    Ok(changed)
}

fn spawn_world_state(composition: Res<WorldComposition>, mut commands: Commands) {
    let mut world_state = commands.spawn(AuthoritativeWorldState);
    configure_replicated_world_state(&mut world_state, composition.occupancy().clone());
}

fn publish_world_occupancy(
    composition: Res<WorldComposition>,
    runtime: Res<WorldRuntimeState>,
    mut world_state: Query<&mut AnchorOccupancy, With<AuthoritativeWorldState>>,
) {
    if !composition.is_changed() {
        return;
    }
    if runtime.applied_generation() != Some(composition.occupancy().generation()) {
        return;
    }
    let Ok(mut replicated) = world_state.single_mut() else {
        warn!(
            "cannot publish world occupancy: expected exactly one authoritative world-state entity"
        );
        return;
    };
    if *replicated != *composition.occupancy() {
        *replicated = composition.occupancy().clone();
    }
}

fn accept_join_requests(
    requests: Query<(Entity, &ServerJoinRequest)>,
    players: Query<&PlayerOwner>,
    mut next_actor_id: ResMut<NextActorId>,
    content: Res<RuntimeContent>,
    health: Res<CharacterHealthCatalog>,
    masses: Res<CharacterMassCatalog>,
    abilities: Res<CharacterAbilityCatalog>,
    exertion: Res<ExertionRules>,
    mut commands: Commands,
) {
    if requests.is_empty() {
        return;
    }
    let mut joined_owners = players.iter().map(|owner| owner.0).collect::<HashSet<_>>();
    for (request_entity, request) in &requests {
        commands.entity(request_entity).despawn();
        if !content.contains_character(&request.character) {
            warn!(owner = request.owner(), character = ?request.character, "ignoring join for unknown character");
            continue;
        }
        if !joined_owners.insert(request.owner()) {
            warn!(owner = request.owner(), "ignoring repeated join request");
            continue;
        }
        let selected = request.character.clone();
        let Some(maximum_health) = health.max_hp(&selected) else {
            warn!(owner = request.owner(), character = ?selected, "ignoring join without derived character health");
            continue;
        };
        let Some(mass) = masses.character(&selected) else {
            warn!(owner = request.owner(), character = ?selected, "ignoring join without derived character mass");
            continue;
        };
        let actor_id = next_actor_id.0;
        let Some(following_id) = actor_id.checked_add(1) else {
            warn!("actor id space exhausted; ignoring join request");
            continue;
        };
        next_actor_id.0 = following_id;
        let spawn = spawn_position(actor_id);
        let mut player = commands.spawn((
            (
                ActorId(actor_id),
                PlayerOwner(request.owner()),
                SelectedCharacter(selected.clone()),
            ),
            (
                MovementIntent::ZERO,
                GazeIntent::ZERO,
                AttackIntent::RELEASED,
                RunIntent::RELEASED,
                DashIntent::RELEASED,
                DeathConfirmIntent::RELEASED,
            ),
            (
                MovementDirection::ZERO,
                MovementVelocity::ZERO,
                StaminaState::full(exertion.default_max_stamina()),
                RunState::default(),
                DashState::default(),
                StatusEffectState::default(),
                CharacterLifeState::Alive,
                DeathConfirmationState::default(),
                RevivalState::IDLE,
                RespawnState::default(),
            ),
            (
                BodyFacing::Authored,
                GazeDirection::RIGHT,
                Position::new(spawn.x, spawn.y),
                CharacterHealth::full(maximum_health),
                mass,
            ),
        ));
        abilities.insert_ability_state(&selected, &mut player);
        configure_replicated_player(&mut player, request);
    }
}

fn spawn_position(actor_id: u64) -> Vec2 {
    const POSITIONS: [Vec2; MAX_CLIENTS] = [
        Vec2::new(-4.0, 0.0),
        Vec2::new(-2.0, 0.0),
        Vec2::ZERO,
        Vec2::new(2.0, 0.0),
        Vec2::new(4.0, 0.0),
    ];
    let index = (actor_id.saturating_sub(1) % POSITIONS.len() as u64) as usize;
    POSITIONS[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_content::WorldCollisionGeometryCatalog;
    use world01_simulation::{WorldColliderGrid, add_world_runtime_rebuild};
    use world01_world_data::{
        AnkhLayout, PlacementRanks, WorldMap, WorldOccupancyRequest, WorldTemplateCatalog,
    };

    #[derive(Resource, Default)]
    struct PublishedOccupancyChanges(u32);

    fn count_published_occupancy_changes(
        published: Query<Ref<AnchorOccupancy>, With<AuthoritativeWorldState>>,
        mut count: ResMut<PublishedOccupancyChanges>,
    ) {
        if published
            .single()
            .is_ok_and(|occupancy| occupancy.is_changed())
        {
            count.0 += 1;
        }
    }

    #[test]
    fn spawn_positions_are_separated_and_repeat_safely() {
        assert_ne!(spawn_position(1), spawn_position(2));
        assert_eq!(spawn_position(1), spawn_position(6));
    }

    #[test]
    fn debug_presets_describe_the_complete_two_anchor_occupancy() {
        let map = WorldMap::load_embedded("overworld01").expect("the embedded Instance is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("the embedded Templates are valid");
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("ankh", 100)])
            .expect("the current embedded Assets have Placement Ranks");
        let mut composition = WorldComposition::new(map, &templates, &ranks)
            .expect("the initial composition is valid");

        assert!(
            apply_world_template_debug_preset(
                &mut composition,
                WorldTemplateDebugPreset::BothAnchors,
                &templates,
                &ranks,
            )
            .expect("both Anchors accept the debug Template")
        );
        assert_eq!(
            composition.occupancy().occupant(FIRST_TEST_ANCHOR_ID),
            Some(TEST_TEMPLATE_SCENE_ID)
        );
        assert_eq!(
            composition.occupancy().occupant(SECOND_TEST_ANCHOR_ID),
            Some(TEST_TEMPLATE_SCENE_ID)
        );

        assert!(
            apply_world_template_debug_preset(
                &mut composition,
                WorldTemplateDebugPreset::FirstAnchor,
                &templates,
                &ranks,
            )
            .expect("the first-Anchor preset is valid")
        );
        assert_eq!(
            composition.occupancy().occupant(FIRST_TEST_ANCHOR_ID),
            Some(TEST_TEMPLATE_SCENE_ID)
        );
        assert_eq!(
            composition.occupancy().occupant(SECOND_TEST_ANCHOR_ID),
            None
        );

        assert!(
            apply_world_template_debug_preset(
                &mut composition,
                WorldTemplateDebugPreset::Empty,
                &templates,
                &ranks,
            )
            .expect("the empty preset is valid")
        );
        assert!(composition.occupancy().occupants().next().is_none());
    }

    #[test]
    fn debug_requests_require_a_joined_owner_and_latest_request_wins() {
        let map = WorldMap::load_embedded("overworld01").expect("the embedded Instance is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("the embedded Templates are valid");
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("ankh", 100)])
            .expect("the current embedded Assets have Placement Ranks");
        let composition = WorldComposition::new(map, &templates, &ranks)
            .expect("the initial composition is valid");
        let mut app = App::new();
        app.insert_resource(composition)
            .insert_resource(templates)
            .insert_resource(ranks)
            .init_resource::<PendingWorldTemplateDebugPreset>()
            .init_resource::<WorldOccupancyRequest>()
            .add_systems(Update, accept_world_template_debug_requests)
            .add_systems(FixedUpdate, submit_pending_world_template_debug_preset);

        let unauthorized = app
            .world_mut()
            .spawn(ServerWorldTemplateDebugRequest::new(
                WorldTemplateDebugPreset::BothAnchors,
                7,
                0,
            ))
            .id();
        app.world_mut().run_schedule(Update);
        assert!(app.world().get_entity(unauthorized).is_err());
        assert_eq!(
            app.world()
                .resource::<PendingWorldTemplateDebugPreset>()
                .latest,
            None
        );

        app.world_mut().spawn(PlayerOwner(7));
        app.world_mut().spawn(ServerWorldTemplateDebugRequest::new(
            WorldTemplateDebugPreset::FirstAnchor,
            7,
            1,
        ));
        app.world_mut().spawn(ServerWorldTemplateDebugRequest::new(
            WorldTemplateDebugPreset::SecondAnchor,
            7,
            2,
        ));
        app.world_mut().run_schedule(Update);
        assert_eq!(
            app.world()
                .resource::<PendingWorldTemplateDebugPreset>()
                .latest,
            Some((2, WorldTemplateDebugPreset::SecondAnchor))
        );
        app.world_mut().run_schedule(FixedUpdate);
        let second = app
            .world()
            .resource::<WorldOccupancyRequest>()
            .latest()
            .expect("the latest authorized request is submitted");
        assert_eq!(second.occupant(FIRST_TEST_ANCHOR_ID), None);
        assert_eq!(
            second.occupant(SECOND_TEST_ANCHOR_ID),
            Some(TEST_TEMPLATE_SCENE_ID)
        );

        app.world_mut().spawn(ServerWorldTemplateDebugRequest::new(
            WorldTemplateDebugPreset::FirstAnchor,
            7,
            3,
        ));
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(FixedUpdate);
        let first = app
            .world()
            .resource::<WorldOccupancyRequest>()
            .latest()
            .expect("a later request supersedes the unaccepted generation");
        assert_eq!(first.generation(), 2);
        assert_eq!(
            first.occupant(FIRST_TEST_ANCHOR_ID),
            Some(TEST_TEMPLATE_SCENE_ID)
        );
        assert_eq!(first.occupant(SECOND_TEST_ANCHOR_ID), None);
    }

    #[test]
    fn world_state_spawns_current_occupancy_and_publishes_only_real_changes() {
        let map = WorldMap::load_embedded("overworld01").expect("the embedded Instance is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("the embedded Templates are valid");
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("ankh", 100)])
            .expect("the current embedded Assets have Placement Ranks");
        let composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the initial composition is valid");
        let initial_occupancy = composition.occupancy().clone();
        let mut authority = composition.clone();
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let collision = WorldCollisionGeometryCatalog::from_content_and_map(&content, &map)
            .expect("embedded world collision is valid");
        let grid = WorldColliderGrid::from_catalog(&collision);
        let ankhs = AnkhLayout::from_map(&map);

        let mut app = App::new();
        app.insert_resource(composition)
            .insert_resource(content)
            .insert_resource(templates.clone())
            .insert_resource(ranks.clone())
            .insert_resource(map)
            .insert_resource(collision)
            .insert_resource(grid)
            .insert_resource(ankhs)
            .init_resource::<PublishedOccupancyChanges>()
            .add_systems(Startup, spawn_world_state)
            .add_systems(
                FixedUpdate,
                (
                    publish_world_occupancy.after(WorldRuntimeSet::Rebuild),
                    count_published_occupancy_changes.after(publish_world_occupancy),
                ),
            );
        add_world_runtime_rebuild(&mut app, FixedUpdate);

        app.world_mut().run_schedule(Startup);
        let published = app
            .world_mut()
            .query_filtered::<&AnchorOccupancy, With<AuthoritativeWorldState>>()
            .single(app.world())
            .expect("exactly one authoritative world-state entity exists");
        assert_eq!(published, &initial_occupancy);

        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(app.world().resource::<PublishedOccupancyChanges>().0, 1);
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(app.world().resource::<PublishedOccupancyChanges>().0, 1);

        authority
            .set_occupant("template_anchor_001", "test_template", &templates, &ranks)
            .expect("the server-owned assignment is valid");
        let expected = authority.occupancy().clone();
        assert!(
            app.world_mut()
                .resource_mut::<WorldOccupancyRequest>()
                .submit(expected.clone())
        );
        app.world_mut().run_schedule(FixedUpdate);

        let published = app
            .world_mut()
            .query_filtered::<&AnchorOccupancy, With<AuthoritativeWorldState>>()
            .single(app.world())
            .expect("the authoritative world-state entity remains unique");
        assert_eq!(published, &expected);
        assert_eq!(app.world().resource::<PublishedOccupancyChanges>().0, 2);
    }

    #[test]
    fn world_state_does_not_publish_an_unapplied_composition_generation() {
        let map = WorldMap::load_embedded("overworld01").expect("the embedded Instance is valid");
        let templates =
            WorldTemplateCatalog::load_embedded().expect("the embedded Templates are valid");
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 20), ("ankh", 100)])
            .expect("the current embedded Assets have Placement Ranks");
        let composition = WorldComposition::new(map, &templates, &ranks)
            .expect("the initial composition is valid");
        let initial_occupancy = composition.occupancy().clone();

        let mut app = App::new();
        app.insert_resource(composition)
            .init_resource::<WorldRuntimeState>()
            .add_systems(Startup, spawn_world_state)
            .add_systems(FixedUpdate, publish_world_occupancy);
        app.world_mut().run_schedule(Startup);

        app.world_mut()
            .resource_mut::<WorldComposition>()
            .set_occupant("template_anchor_001", "test_template", &templates, &ranks)
            .expect("the direct mutation creates an unapplied generation");
        app.world_mut().run_schedule(FixedUpdate);

        let published = app
            .world_mut()
            .query_filtered::<&AnchorOccupancy, With<AuthoritativeWorldState>>()
            .single(app.world())
            .expect("exactly one authoritative world-state entity exists");
        assert_eq!(published, &initial_occupancy);
    }
}

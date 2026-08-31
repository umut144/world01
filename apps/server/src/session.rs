use std::collections::HashSet;

use bevy::{log::warn, prelude::*};
use world01_content::{CharacterHealthCatalog, RuntimeContent};
use world01_network::{
    MAX_CLIENTS, ServerJoinRequest, ServerNetworkSet, configure_replicated_player,
};
use world01_simulation::{CharacterArchetypeCatalog, CharacterMassCatalog, LocomotionRules};
use world01_world_data::{
    ActorId, Ankh, AnkhLayout, AttackIntent, BodyFacing, CharacterHealth, CharacterLifeState,
    DashIntent, DashState, DeathConfirmIntent, DeathConfirmationState, GazeDirection, GazeIntent,
    MovementDirection, MovementIntent, MovementVelocity, PlayerOwner, Position, RespawnState,
    RevivalState, RunIntent, RunState, SelectedCharacter, StaminaState, StatusEffectState,
};

#[derive(Resource, Debug)]
struct NextActorId(u64);

impl Default for NextActorId {
    fn default() -> Self {
        Self(1)
    }
}

pub struct ServerSessionPlugin;

impl Plugin for ServerSessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextActorId>()
            .add_systems(Startup, spawn_room_ankhs)
            .add_systems(
                Update,
                accept_join_requests.after(ServerNetworkSet::ReceiveRequests),
            );
    }
}

fn spawn_room_ankhs(layout: Res<AnkhLayout>, mut commands: Commands) {
    for (index, position) in layout.positions.iter().copied().enumerate() {
        commands.spawn((Ankh::new(index as u32), position));
    }
}

fn accept_join_requests(
    requests: Query<(Entity, &ServerJoinRequest)>,
    players: Query<&PlayerOwner>,
    mut next_actor_id: ResMut<NextActorId>,
    content: Res<RuntimeContent>,
    health: Res<CharacterHealthCatalog>,
    masses: Res<CharacterMassCatalog>,
    archetypes: Res<CharacterArchetypeCatalog>,
    locomotion: Res<LocomotionRules>,
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
                StaminaState::full(locomotion.default_max_stamina()),
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
        archetypes.insert_ability_state(&selected, &mut player);
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

    #[test]
    fn spawn_positions_are_separated_and_repeat_safely() {
        assert_ne!(spawn_position(1), spawn_position(2));
        assert_eq!(spawn_position(1), spawn_position(6));
    }
}

use std::collections::HashSet;

use bevy::{log::warn, prelude::*};
use game01_content::{CharacterHealthCatalog, RuntimeContent};
use game01_network::{
    MAX_CLIENTS, ServerJoinRequest, ServerNetworkSet, configure_replicated_player,
};
use game01_world_data::{
    AttackIntent, BodyFacing, CharacterHealth, GazeDirection, GazeIntent, HammerAttackState,
    MovementDirection, MovementIntent, MovementSpeedScale, PlayerId, PlayerOwner, Position,
    SelectedCharacter, StartingRoomGrid, WeaponAimState,
};

#[derive(Resource, Debug)]
struct NextPlayerId(u64);

impl Default for NextPlayerId {
    fn default() -> Self {
        Self(1)
    }
}

pub struct ServerSessionPlugin;

impl Plugin for ServerSessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextPlayerId>().add_systems(
            Update,
            accept_join_requests.after(ServerNetworkSet::ReceiveRequests),
        );
    }
}

fn accept_join_requests(
    requests: Query<(Entity, &ServerJoinRequest)>,
    players: Query<&PlayerOwner>,
    mut next_player_id: ResMut<NextPlayerId>,
    room_grid: Res<StartingRoomGrid>,
    content: Res<RuntimeContent>,
    health: Res<CharacterHealthCatalog>,
    mut commands: Commands,
) {
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
        let player_id = next_player_id.0;
        let Some(following_id) = player_id.checked_add(1) else {
            warn!("player id space exhausted; ignoring join request");
            continue;
        };
        next_player_id.0 = following_id;
        let spawn = spawn_position(player_id);
        let selected = request.character.clone();
        let maximum_health = health.max_hp(&selected).unwrap_or(140.0);
        let mut player = commands.spawn((
            PlayerId(player_id),
            PlayerOwner(request.owner()),
            SelectedCharacter(selected.clone()),
            MovementIntent::ZERO,
            GazeIntent::ZERO,
            AttackIntent::RELEASED,
            MovementDirection::ZERO,
            MovementSpeedScale::default(),
            BodyFacing::Authored,
            GazeDirection::RIGHT,
            Position::new(spawn.x, spawn.y),
            room_grid.starting_room(),
            CharacterHealth::full(maximum_health),
        ));
        if selected.0 == "hammerer" {
            player.insert((WeaponAimState::RIGHT, HammerAttackState::IDLE));
        }
        configure_replicated_player(&mut player, request);
    }
}

fn spawn_position(player_id: u64) -> Vec2 {
    const POSITIONS: [Vec2; MAX_CLIENTS] = [
        Vec2::new(-4.0, 0.0),
        Vec2::new(-2.0, 0.0),
        Vec2::ZERO,
        Vec2::new(2.0, 0.0),
        Vec2::new(4.0, 0.0),
    ];
    let index = (player_id.saturating_sub(1) % POSITIONS.len() as u64) as usize;
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

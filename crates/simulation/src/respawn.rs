use bevy::prelude::Vec2;
use world01_content::CharacterHurtGeometryCatalog;
use world01_world_data::{Ankh, BodyFacing, CharacterId, CharacterLifeState, WorldPosition};

use crate::spatial::overlap::{components_overlap, hurt_transform};

const MAX_CANDIDATES_PER_ANKH: u32 = 64;

#[derive(Clone)]
pub struct RespawnActor {
    pub actor_id: u64,
    pub character: Option<CharacterId>,
    pub position: Option<WorldPosition>,
    pub facing: Option<BodyFacing>,
    pub life: CharacterLifeState,
}

pub fn choose_respawn_position(
    actor_id: u64,
    respawn_count: u32,
    fallback: WorldPosition,
    character: Option<&CharacterId>,
    facing: Option<BodyFacing>,
    radius: f32,
    ankhs: &[(Ankh, WorldPosition)],
    actors: &[RespawnActor],
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
) -> WorldPosition {
    let Some(character) = character else {
        return fallback;
    };
    let Some(facing) = facing else {
        return fallback;
    };
    let Some(hurt_geometry) = hurt_geometry else {
        return fallback;
    };
    let Some(target_geometry) = hurt_geometry.character(character) else {
        return fallback;
    };

    let mut ordered_ankhs = ankhs.to_vec();
    ordered_ankhs.sort_by(
        |(first_ankh, first_position), (second_ankh, second_position)| {
            distance_squared(*first_position, fallback)
                .total_cmp(&distance_squared(*second_position, fallback))
                .then_with(|| first_ankh.index.cmp(&second_ankh.index))
        },
    );

    for (ankh, position) in ordered_ankhs {
        for attempt in 0..MAX_CANDIDATES_PER_ANKH {
            let candidate = candidate_position(
                position,
                actor_id,
                respawn_count,
                ankh.index,
                attempt,
                radius,
            );
            if !candidate_is_blocked(
                candidate,
                actor_id,
                facing,
                target_geometry,
                actors,
                hurt_geometry,
            ) {
                return candidate;
            }
        }
    }

    fallback
}

fn candidate_is_blocked(
    candidate: WorldPosition,
    target_actor_id: u64,
    facing: BodyFacing,
    target_geometry: &world01_content::CharacterHurtGeometry,
    actors: &[RespawnActor],
    hurt_geometry: &CharacterHurtGeometryCatalog,
) -> bool {
    let target_transform = hurt_transform(target_geometry, candidate.horizontal(), facing);
    actors.iter().any(|actor| {
        if actor.life != CharacterLifeState::Alive || actor.actor_id == target_actor_id {
            return false;
        }
        let (Some(other_character), Some(other_position), Some(other_facing)) =
            (&actor.character, actor.position, actor.facing)
        else {
            return false;
        };
        let Some(other_geometry) = hurt_geometry.character(other_character) else {
            return false;
        };
        let other_transform =
            hurt_transform(other_geometry, other_position.horizontal(), other_facing);
        target_geometry.components.iter().any(|target_component| {
            other_geometry.components.iter().any(|other_component| {
                components_overlap(
                    target_component,
                    target_transform,
                    other_component,
                    other_transform,
                )
            })
        })
    })
}

fn candidate_position(
    anchor: WorldPosition,
    actor_id: u64,
    respawn_count: u32,
    ankh_index: u32,
    attempt: u32,
    radius: f32,
) -> WorldPosition {
    let seed = mix64(
        actor_id
            ^ (u64::from(respawn_count) << 32)
            ^ (u64::from(ankh_index) << 16)
            ^ u64::from(attempt),
    );
    let angle = unit_interval(seed) * std::f32::consts::TAU;
    let distance = unit_interval(mix64(seed)).sqrt() * radius.max(0.0);
    WorldPosition::new(
        anchor.x + distance * angle.cos(),
        anchor.y + distance * angle.sin(),
        anchor.elevation_meters,
    )
}

fn distance_squared(first: WorldPosition, second: WorldPosition) -> f32 {
    Vec2::new(first.x - second.x, first.y - second.y).length_squared()
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit_interval(value: u64) -> f32 {
    (value >> 40) as f32 / (1_u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use world01_content::{CharacterHurtGeometryCatalog, RuntimeContent};
    use world01_design::load_embedded as load_game_design;

    fn hurt_geometry() -> CharacterHurtGeometryCatalog {
        CharacterHurtGeometryCatalog::from_content(
            &RuntimeContent::load_embedded().expect("embedded content is valid"),
            &load_game_design()
                .expect("embedded game design parses")
                .hurt,
        )
        .expect("embedded hurt geometry is valid")
    }

    #[test]
    fn nearest_ankh_is_preferred_and_candidates_are_deterministic() {
        let ankhs = [
            (Ankh::new(0), WorldPosition::new(10.0, 0.0, 1.0)),
            (Ankh::new(1), WorldPosition::new(2.0, 0.0, 2.0)),
        ];
        let geometry = hurt_geometry();
        let character = CharacterId("hammerer".into());
        let result = choose_respawn_position(
            7,
            1,
            WorldPosition::ZERO,
            Some(&character),
            Some(BodyFacing::Authored),
            4.0,
            &ankhs,
            &[],
            Some(&geometry),
        );
        let repeated = choose_respawn_position(
            7,
            1,
            WorldPosition::ZERO,
            Some(&character),
            Some(BodyFacing::Authored),
            4.0,
            &ankhs,
            &[],
            Some(&geometry),
        );

        assert_eq!(result, repeated);
        assert_eq!(result.elevation_meters, 2.0);
        assert!(Vec2::new(result.x - 2.0, result.y).length() <= 4.0);
        assert!(Vec2::new(result.x - 10.0, result.y).length() > 4.0);
    }

    #[test]
    fn fully_blocked_nearest_ankh_falls_through_to_the_next_ankh() {
        let geometry = hurt_geometry();
        let nearest = (Ankh::new(0), WorldPosition::ZERO);
        let next = (Ankh::new(1), WorldPosition::new(10.0, 0.0, 3.0));
        let blockers = (0..64)
            .map(|attempt| RespawnActor {
                actor_id: 100 + u64::from(attempt),
                character: Some(CharacterId("hammerer".into())),
                position: Some(candidate_position(
                    nearest.1,
                    7,
                    1,
                    nearest.0.index,
                    attempt,
                    4.0,
                )),
                facing: Some(BodyFacing::Authored),
                life: CharacterLifeState::Alive,
            })
            .collect::<Vec<_>>();

        let result = choose_respawn_position(
            7,
            1,
            WorldPosition::new(1.0, 0.0, 1.0),
            Some(&CharacterId("hammerer".into())),
            Some(BodyFacing::Authored),
            4.0,
            &[nearest, next],
            &blockers,
            Some(&geometry),
        );

        assert!(Vec2::new(result.x - 10.0, result.y).length() <= 4.0);
        assert_eq!(result.elevation_meters, 3.0);
    }

    #[test]
    fn no_ankh_falls_back_to_the_actors_current_position() {
        let result = choose_respawn_position(
            7,
            1,
            WorldPosition::new(3.0, -2.0, 7.0),
            None,
            None,
            4.0,
            &[],
            &[],
            None,
        );

        assert_eq!(result, WorldPosition::new(3.0, -2.0, 7.0));
    }
}

use bevy::prelude::Vec2;
use world01_content::CharacterHurtGeometryCatalog;
use world01_world_data::{Ankh, BodyFacing, CharacterId, CharacterLifeState, Position};

use crate::combat::overlap::{components_overlap, hurt_transform};

const MAX_CANDIDATES_PER_ANKH: u32 = 64;

#[derive(Clone)]
pub struct RespawnPlayer {
    pub player_id: u64,
    pub character: Option<CharacterId>,
    pub position: Option<Position>,
    pub facing: Option<BodyFacing>,
    pub life: CharacterLifeState,
}

pub fn choose_respawn_position(
    player_id: u64,
    respawn_count: u32,
    fallback: Position,
    character: Option<&CharacterId>,
    facing: Option<BodyFacing>,
    radius: f32,
    ankhs: &[(Ankh, Position)],
    players: &[RespawnPlayer],
    hurt_geometry: Option<&CharacterHurtGeometryCatalog>,
) -> Position {
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
                player_id,
                respawn_count,
                ankh.index,
                attempt,
                radius,
            );
            if !candidate_is_blocked(
                candidate,
                player_id,
                facing,
                target_geometry,
                players,
                hurt_geometry,
            ) {
                return candidate;
            }
        }
    }

    fallback
}

fn candidate_is_blocked(
    candidate: Position,
    target_player_id: u64,
    facing: BodyFacing,
    target_geometry: &world01_content::CharacterHurtGeometry,
    players: &[RespawnPlayer],
    hurt_geometry: &CharacterHurtGeometryCatalog,
) -> bool {
    let target_transform = hurt_transform(target_geometry, candidate, facing);
    players.iter().any(|player| {
        if player.life != CharacterLifeState::Alive || player.player_id == target_player_id {
            return false;
        }
        let (Some(other_character), Some(other_position), Some(other_facing)) =
            (&player.character, player.position, player.facing)
        else {
            return false;
        };
        let Some(other_geometry) = hurt_geometry.character(other_character) else {
            return false;
        };
        let other_transform = hurt_transform(other_geometry, other_position, other_facing);
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
    anchor: Position,
    player_id: u64,
    respawn_count: u32,
    ankh_index: u32,
    attempt: u32,
    radius: f32,
) -> Position {
    let seed = mix64(
        player_id
            ^ (u64::from(respawn_count) << 32)
            ^ (u64::from(ankh_index) << 16)
            ^ u64::from(attempt),
    );
    let angle = unit_interval(seed) * std::f32::consts::TAU;
    let distance = unit_interval(mix64(seed)).sqrt() * radius.max(0.0);
    Position::new(
        anchor.x + distance * angle.cos(),
        anchor.y + distance * angle.sin(),
    )
}

fn distance_squared(first: Position, second: Position) -> f32 {
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

    fn hurt_geometry() -> CharacterHurtGeometryCatalog {
        CharacterHurtGeometryCatalog::from_content(
            &RuntimeContent::load_embedded().expect("embedded content is valid"),
        )
        .expect("embedded hurt geometry is valid")
    }

    #[test]
    fn nearest_ankh_is_preferred_and_candidates_are_deterministic() {
        let ankhs = [
            (Ankh::new(0), Position::new(10.0, 0.0)),
            (Ankh::new(1), Position::new(2.0, 0.0)),
        ];
        let geometry = hurt_geometry();
        let character = CharacterId("hammerer".into());
        let result = choose_respawn_position(
            7,
            1,
            Position::ZERO,
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
            Position::ZERO,
            Some(&character),
            Some(BodyFacing::Authored),
            4.0,
            &ankhs,
            &[],
            Some(&geometry),
        );

        assert_eq!(result, repeated);
        assert!(Vec2::new(result.x - 2.0, result.y).length() <= 4.0);
        assert!(Vec2::new(result.x - 10.0, result.y).length() > 4.0);
    }

    #[test]
    fn fully_blocked_nearest_ankh_falls_through_to_the_next_ankh() {
        let geometry = hurt_geometry();
        let nearest = (Ankh::new(0), Position::ZERO);
        let next = (Ankh::new(1), Position::new(10.0, 0.0));
        let blockers = (0..64)
            .map(|attempt| RespawnPlayer {
                player_id: 100 + u64::from(attempt),
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
            Position::new(1.0, 0.0),
            Some(&CharacterId("hammerer".into())),
            Some(BodyFacing::Authored),
            4.0,
            &[nearest, next],
            &blockers,
            Some(&geometry),
        );

        assert!(Vec2::new(result.x - 10.0, result.y).length() <= 4.0);
    }

    #[test]
    fn no_ankh_falls_back_to_the_players_current_position() {
        let result = choose_respawn_position(
            7,
            1,
            Position::new(3.0, -2.0),
            None,
            None,
            4.0,
            &[],
            &[],
            None,
        );

        assert_eq!(result, Position::new(3.0, -2.0));
    }
}

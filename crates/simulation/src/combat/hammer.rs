use std::{error::Error, fmt};

use bevy::prelude::{Query, Res, Resource, Vec2};
use game01_content::HammerCombatGeometry;
use game01_design::HammerDesign;
use game01_world_data::{
    AttackIntent, GazeDirection, HammerAttackPhase, HammerAttackState, Position, SelectedCharacter,
    WeaponAimState,
};

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct HammerAttackRules {
    maximum_charge_ticks: u32,
    grip_reach_ticks: u32,
    swing_ticks: u32,
    embedded_ticks: u32,
    recovery_ticks: u32,
}

impl HammerAttackRules {
    pub fn from_design(
        ticks_per_second: u32,
        design: &HammerDesign,
    ) -> Result<Self, HammerAttackConfigError> {
        if !design.is_valid() {
            return Err(HammerAttackConfigError);
        }
        if ticks_per_second == 0 {
            return Err(HammerAttackConfigError);
        }
        let ticks = |seconds: f32| (seconds * ticks_per_second as f32).round().max(1.0) as u32;
        Ok(Self {
            maximum_charge_ticks: ticks(design.maximum_charge_seconds),
            grip_reach_ticks: ticks(design.grip_reach_seconds),
            swing_ticks: ticks(design.swing_seconds),
            embedded_ticks: ticks(design.embedded_seconds),
            recovery_ticks: ticks(design.recovery_seconds),
        })
    }

    pub fn maximum_charge_ticks(self) -> u32 {
        self.maximum_charge_ticks
    }

    pub fn grip_reach_ticks(self) -> u32 {
        self.grip_reach_ticks
    }

    pub fn swing_ticks(self) -> u32 {
        self.swing_ticks
    }

    pub fn embedded_ticks(self) -> u32 {
        self.embedded_ticks
    }

    pub fn recovery_ticks(self) -> u32 {
        self.recovery_ticks
    }

    pub fn grip_progress(self, charge_ticks: f32) -> f32 {
        charge_ticks.clamp(0.0, self.grip_reach_ticks as f32) / self.grip_reach_ticks as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HammerAttackConfigError;

impl fmt::Display for HammerAttackConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Hammer attack timings must be valid")
    }
}

impl Error for HammerAttackConfigError {}

pub fn constrain_embedded_hammer_reach(
    hammer_geometry: Res<HammerCombatGeometry>,
    mut players: Query<(&HammerAttackState, &mut Position)>,
) {
    for (attack, mut position) in &mut players {
        if attack.phase != HammerAttackPhase::Embedded {
            continue;
        }
        let constrained = constrain_embedded_position(
            Vec2::new(position.x, position.y),
            Vec2::new(attack.impact_point.x, attack.impact_point.y),
            hammer_geometry.socket_offset(),
            hammer_geometry.maximum_reach(),
        );
        *position = Position::new(constrained.x, constrained.y);
    }
}

pub(crate) fn constrain_embedded_position(
    proposed_player_position: Vec2,
    planted_head: Vec2,
    socket_offset: Vec2,
    maximum_reach: f32,
) -> Vec2 {
    let proposed_socket = proposed_player_position + socket_offset;
    let head_to_socket = proposed_socket - planted_head;
    if !head_to_socket.is_finite()
        || !maximum_reach.is_finite()
        || maximum_reach <= 0.0
        || head_to_socket.length_squared() <= maximum_reach * maximum_reach
    {
        return proposed_player_position;
    }
    planted_head + head_to_socket.normalize() * maximum_reach - socket_offset
}

pub fn advance_hammer_attacks(
    rules: Res<HammerAttackRules>,
    hammer_geometry: Res<HammerCombatGeometry>,
    mut players: Query<(
        &SelectedCharacter,
        &AttackIntent,
        &WeaponAimState,
        &Position,
        &mut HammerAttackState,
    )>,
) {
    for (character, attack, weapon_aim, position, mut state) in &mut players {
        if character.0.0 != "hammerer" {
            *state = HammerAttackState::IDLE;
            continue;
        }
        let weapon_aim = valid_direction(weapon_aim.direction());
        match state.phase {
            HammerAttackPhase::Idle => {
                if attack.pressed {
                    state.phase = HammerAttackPhase::Charging;
                    state.direction = weapon_aim.unwrap_or(GazeDirection::ZERO);
                    state.phase_ticks = 0;
                    state.charge_ticks = 0;
                }
            }
            HammerAttackPhase::Charging => {
                if let Some(weapon_aim) = weapon_aim {
                    state.direction = weapon_aim;
                }
                if attack.pressed {
                    state.phase_ticks = state
                        .phase_ticks
                        .saturating_add(1)
                        .min(rules.maximum_charge_ticks);
                    state.charge_ticks = state
                        .charge_ticks
                        .saturating_add(1)
                        .min(rules.maximum_charge_ticks);
                } else if valid_direction(state.direction).is_some() {
                    state.phase = HammerAttackPhase::Swing;
                    state.phase_ticks = 0;
                } else {
                    *state = HammerAttackState::IDLE;
                }
            }
            HammerAttackPhase::Swing => {
                let next_tick = state.phase_ticks.saturating_add(1);
                if next_tick >= rules.swing_ticks {
                    let Some(direction) = valid_direction(state.direction) else {
                        *state = HammerAttackState::IDLE;
                        continue;
                    };
                    let direction = Vec2::new(direction.x, direction.y);
                    let socket =
                        Vec2::new(position.x, position.y) + hammer_geometry.socket_offset();
                    let impact = socket
                        + direction
                            * hammer_geometry
                                .attack_radius(rules.grip_progress(state.charge_ticks as f32));
                    state.phase = HammerAttackPhase::Embedded;
                    state.phase_ticks = 0;
                    state.impact_point = Position::new(impact.x, impact.y);
                } else {
                    state.phase_ticks = next_tick;
                }
            }
            HammerAttackPhase::Embedded => {
                let next_tick = state.phase_ticks.saturating_add(1);
                if next_tick >= rules.embedded_ticks {
                    state.phase = HammerAttackPhase::Recovery;
                    state.phase_ticks = 0;
                } else {
                    state.phase_ticks = next_tick;
                }
            }
            HammerAttackPhase::Recovery => {
                let next_tick = state.phase_ticks.saturating_add(1);
                if next_tick >= rules.recovery_ticks {
                    *state = HammerAttackState::IDLE;
                } else {
                    state.phase_ticks = next_tick;
                }
            }
        }
    }
}

fn valid_direction(direction: GazeDirection) -> Option<GazeDirection> {
    let direction = Vec2::new(direction.x, direction.y);
    (direction.is_finite() && direction != Vec2::ZERO).then(|| {
        let direction = direction.normalize();
        GazeDirection::new(direction.x, direction.y)
    })
}

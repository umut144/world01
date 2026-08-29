use std::{collections::HashSet, error::Error, fmt};

use bevy::prelude::{Entity, Query, Res, Resource, Vec2};
use world01_content::{CharacterHurtGeometryCatalog, HammerCombatGeometry};
use world01_design::{HammerDesign, HammerStrikeDesign};
use world01_world_data::{
    AttackIntent, BodyFacing, CharacterHealth, CharacterLifeState, DashState, GazeDirection,
    HammerAttackPhase, HammerAttackState, Position, SelectedCharacter, StatusEffectState,
    WeaponAimState,
};

#[cfg(test)]
use super::overlap::triangles_overlap;
use super::overlap::{components_overlap, hurt_transform, rotate};

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

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HammerStrikeRules {
    base_damage: f32,
    charge_step_ticks: u32,
    charge_damage_ratio_per_step: f32,
    maximum_charge_steps: u32,
    stunned_duration_ticks: u32,
    stunned_components: HashSet<String>,
}

impl HammerStrikeRules {
    pub fn from_design(
        ticks_per_second: u32,
        hammer: &HammerDesign,
        strike: &HammerStrikeDesign,
    ) -> Result<Self, HammerStrikeConfigError> {
        if ticks_per_second == 0 || !strike.is_valid() {
            return Err(HammerStrikeConfigError);
        }
        let charge_step_ticks = (strike.charge_step_seconds * ticks_per_second as f32)
            .round()
            .max(1.0) as u32;
        let maximum_charge_ticks =
            (hammer.maximum_charge_seconds * ticks_per_second as f32).round() as u32;
        let stunned_effects = strike
            .component_effects
            .iter()
            .filter_map(|component| {
                component
                    .effects
                    .iter()
                    .find(|effect| effect.name_key == "STUNNED")
                    .map(|effect| (component.component_name.clone(), effect.duration_seconds))
            })
            .collect::<Vec<_>>();
        let stunned_duration_seconds = stunned_effects
            .first()
            .map(|(_, duration)| *duration)
            .unwrap_or_default();
        if !stunned_effects
            .iter()
            .all(|(_, duration)| (*duration - stunned_duration_seconds).abs() <= f32::EPSILON)
        {
            return Err(HammerStrikeConfigError);
        }
        Ok(Self {
            base_damage: strike.base_damage,
            charge_step_ticks,
            charge_damage_ratio_per_step: strike.charge_damage_percent_per_step / 100.0,
            maximum_charge_steps: maximum_charge_ticks / charge_step_ticks,
            stunned_duration_ticks: (stunned_duration_seconds * ticks_per_second as f32)
                .round()
                .max(1.0) as u32,
            stunned_components: stunned_effects
                .into_iter()
                .map(|(component, _)| component)
                .collect(),
        })
    }

    pub fn damage_for_charge(&self, charge_ticks: u32) -> f32 {
        let completed_steps =
            (charge_ticks / self.charge_step_ticks).min(self.maximum_charge_steps);
        self.base_damage * (1.0 + self.charge_damage_ratio_per_step * completed_steps as f32)
    }

    pub fn stunned_duration_ticks(&self) -> u32 {
        self.stunned_duration_ticks
    }

    fn stuns_from_component(&self, component_name: &str) -> bool {
        self.stunned_components.contains(component_name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HammerStrikeConfigError;

impl fmt::Display for HammerStrikeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HammerStrike configuration is invalid")
    }
}

impl Error for HammerStrikeConfigError {}

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

pub fn apply_hammer_strike_damage(
    rules: Res<HammerStrikeRules>,
    hammer_geometry: Res<HammerCombatGeometry>,
    hurt_geometry: Res<CharacterHurtGeometryCatalog>,
    mut players: bevy::ecs::system::ParamSet<(
        Query<(&HammerAttackState, &SelectedCharacter)>,
        Query<(
            Entity,
            &SelectedCharacter,
            &Position,
            &BodyFacing,
            &DashState,
            Option<&CharacterLifeState>,
            Option<&mut HammerAttackState>,
            &mut CharacterHealth,
            &mut StatusEffectState,
        )>,
    )>,
) {
    let impacts = {
        let attackers = players.p0();
        attackers
            .iter()
            .filter(|(attack, character)| {
                character.0.0 == "hammerer"
                    && attack.phase == HammerAttackPhase::Embedded
                    && attack.phase_ticks == 0
            })
            .map(|(attack, _)| *attack)
            .collect::<Vec<_>>()
    };

    if impacts.is_empty() {
        return;
    }

    let mut targets = players.p1();
    for attack in impacts {
        let Some(attack_direction) = valid_direction(attack.direction) else {
            continue;
        };
        let Some(attack_transform) = attack_transform(
            &hammer_geometry,
            Vec2::new(attack_direction.x, attack_direction.y),
            Vec2::new(attack.impact_point.x, attack.impact_point.y),
        ) else {
            continue;
        };
        for (_, character, position, facing, dash, life, attack_state, mut health, mut status) in
            &mut targets
        {
            if dash.invulnerable || life.is_some_and(|life| !life.is_alive()) {
                continue;
            }
            let Some(hurt) = hurt_geometry.character(&character.0) else {
                continue;
            };
            let target_transform = hurt_transform(hurt, *position, *facing);
            let mut hit = false;
            let mut stunned = false;
            for attack_component in hammer_geometry.attack_components() {
                for hurt_component in &hurt.components {
                    if components_overlap(
                        attack_component,
                        attack_transform,
                        hurt_component,
                        target_transform,
                    ) {
                        hit = true;
                        stunned |= rules.stuns_from_component(&attack_component.name);
                        break;
                    }
                }
            }
            if !hit {
                continue;
            }
            health.current =
                (health.current - rules.damage_for_charge(attack.charge_ticks)).max(0.0);
            if stunned {
                status.stunned_ticks = status.stunned_ticks.max(rules.stunned_duration_ticks());
                if let Some(mut attack_state) = attack_state {
                    *attack_state = HammerAttackState::IDLE;
                }
            }
        }
    }
}

fn attack_transform(
    geometry: &HammerCombatGeometry,
    direction: Vec2,
    impact_point: Vec2,
) -> Option<super::overlap::GeometryTransform> {
    let source = geometry.attack_point() - geometry.secondary_grip();
    (source.is_finite() && source.length_squared() > f32::EPSILON).then(|| {
        let angle = direction.to_angle() - source.to_angle();
        let axis_x = Vec2::from_angle(angle);
        super::overlap::GeometryTransform {
            origin: impact_point - rotate(axis_x, geometry.attack_point()),
            axis_x,
            axis_y: axis_x.perp(),
        }
    })
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
        Option<&StatusEffectState>,
        Option<&CharacterLifeState>,
        &mut HammerAttackState,
    )>,
) {
    for (character, attack, weapon_aim, position, status, life, mut state) in &mut players {
        if character.0.0 != "hammerer" {
            *state = HammerAttackState::IDLE;
            continue;
        }
        if status.is_some_and(|status| status.blocks_all_input())
            || life.is_some_and(|life| !life.is_alive())
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Update};
    use world01_configs::load_embedded;
    use world01_content::RuntimeContent;
    use world01_design::load_embedded as load_game_design;

    #[test]
    fn hammer_strike_damage_reaches_forty_at_full_charge() {
        let config = load_embedded().expect("embedded config parses");
        let design = load_game_design().expect("embedded game design parses");
        let rules = HammerStrikeRules::from_design(
            config.simulation.ticks_per_second,
            &design.hammer,
            &design.hammer_strike,
        )
        .expect("embedded HammerStrike design is valid");

        assert_eq!(rules.damage_for_charge(0), 20.0);
        assert_eq!(rules.damage_for_charge(30), 22.0);
        assert_eq!(rules.damage_for_charge(300), 40.0);
        assert_eq!(rules.damage_for_charge(360), 40.0);
        assert_eq!(rules.stunned_duration_ticks(), 240);
    }

    #[test]
    fn polygon_overlap_accepts_intersection_and_rejects_separation() {
        let first = [Vec2::ZERO, Vec2::X, Vec2::Y];
        let overlapping = [
            Vec2::new(0.25, 0.25),
            Vec2::new(1.25, 0.25),
            Vec2::new(0.25, 1.25),
        ];
        let separated = [
            Vec2::new(2.0, 2.0),
            Vec2::new(3.0, 2.0),
            Vec2::new(2.0, 3.0),
        ];

        assert!(triangles_overlap(first, overlapping));
        assert!(!triangles_overlap(first, separated));
    }

    #[test]
    fn stunned_interrupts_an_existing_hammer_attack() {
        let config = load_embedded().expect("embedded config parses");
        let design = load_game_design().expect("embedded game design parses");
        let content = RuntimeContent::load_embedded().expect("embedded content is valid");
        let rules =
            HammerAttackRules::from_design(config.simulation.ticks_per_second, &design.hammer)
                .expect("embedded Hammer attack design is valid");
        let geometry =
            HammerCombatGeometry::from_content(&content, &design.hammer.attack_components)
                .expect("embedded Hammer geometry is valid");
        let mut app = App::new();
        app.insert_resource(rules)
            .insert_resource(geometry)
            .add_systems(Update, advance_hammer_attacks);
        let player = app
            .world_mut()
            .spawn((
                SelectedCharacter(world01_world_data::CharacterId("hammerer".into())),
                AttackIntent::PRESSED,
                WeaponAimState::RIGHT,
                Position::ZERO,
                StatusEffectState {
                    stunned_ticks: 1,
                    ..Default::default()
                },
                HammerAttackState {
                    phase: HammerAttackPhase::Charging,
                    direction: GazeDirection::RIGHT,
                    phase_ticks: 10,
                    charge_ticks: 10,
                    impact_point: Position::ZERO,
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<HammerAttackState>(player),
            Some(&HammerAttackState::IDLE)
        );
    }
}

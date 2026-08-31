use std::collections::HashMap;

use bevy::prelude::*;
use world01_simulation::{MageAttackRules, visible_beam_segment};
use world01_world_data::{EyeBeamState, MageAttackPhase, MageAttackState};

use crate::eyes::EyePupil;

const IDLE_PUPIL_COLOR: Color = Color::srgb(0.01, 0.008, 0.01);
const CHARGED_PUPIL_COLOR: Color = Color::srgb(0.95, 0.015, 0.025);
const BEAM_COLOR: Color = Color::srgb(1.0, 0.015, 0.025);
const BEAM_PRESENTATION_LAYER: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MageBeamSide {
    Left,
    Right,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct MageBeamVisual {
    owner: Entity,
    side: MageBeamSide,
}

pub(crate) fn apply_mage_eye_charge(
    rules: Res<MageAttackRules>,
    attacks: Query<&MageAttackState>,
    pupils: Query<(&EyePupil, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (pupil, material_handle) in &pupils {
        let progress = attacks
            .get(pupil.owner)
            .ok()
            .filter(|attack| attack.phase == MageAttackPhase::Charging)
            .map(|attack| rules.charge_progress(attack.charge_ticks))
            .unwrap_or_default();
        let Some(mut material) = materials.get_mut(&material_handle.0) else {
            continue;
        };
        material.color = pupil_color(progress);
    }
}

fn pupil_color(progress: f32) -> Color {
    let progress = progress.clamp(0.0, 1.0);
    let idle = IDLE_PUPIL_COLOR.to_srgba();
    let charged = CHARGED_PUPIL_COLOR.to_srgba();
    Color::srgba(
        idle.red + (charged.red - idle.red) * progress,
        idle.green + (charged.green - idle.green) * progress,
        idle.blue + (charged.blue - idle.blue) * progress,
        1.0,
    )
}

pub(crate) fn sync_mage_beam_visuals(
    mut commands: Commands,
    rules: Res<MageAttackRules>,
    attacks: Query<(Entity, &MageAttackState)>,
    existing: Query<(Entity, &MageBeamVisual)>,
) {
    let mut visuals = existing
        .iter()
        .map(|(entity, visual)| ((visual.owner, visual.side), entity))
        .collect::<HashMap<_, _>>();

    for (owner, _) in &attacks {
        for side in [MageBeamSide::Left, MageBeamSide::Right] {
            visuals.entry((owner, side)).or_insert_with(|| {
                commands
                    .spawn((
                        MageBeamVisual { owner, side },
                        Sprite::from_color(BEAM_COLOR, Vec2::ONE),
                        Transform::from_xyz(0.0, 0.0, BEAM_PRESENTATION_LAYER),
                        Visibility::Hidden,
                    ))
                    .id()
            });
        }
    }

    for (entity, visual) in &existing {
        let Ok((_, state)) = attacks.get(visual.owner) else {
            commands.entity(entity).despawn();
            continue;
        };
        let beam = match visual.side {
            MageBeamSide::Left => state.left_beam,
            MageBeamSide::Right => state.right_beam,
        };
        let Some((start, end)) = visible_beam_segment(&rules, state, beam) else {
            commands.entity(entity).insert(Visibility::Hidden);
            continue;
        };
        let Some(transform) = beam_transform(start, end, beam) else {
            commands.entity(entity).insert(Visibility::Hidden);
            continue;
        };
        commands
            .entity(entity)
            .insert((transform, Visibility::Visible));
    }
}

fn beam_transform(start: Vec2, end: Vec2, beam: EyeBeamState) -> Option<Transform> {
    let delta = end - start;
    let length = delta.length();
    (length.is_finite() && length > f32::EPSILON && beam.width.is_finite() && beam.width > 0.0)
        .then(|| {
            Transform::from_translation(((start + end) * 0.5).extend(BEAM_PRESENTATION_LAYER))
                .with_rotation(Quat::from_rotation_z(delta.to_angle()))
                .with_scale(Vec3::new(length, beam.width, 1.0))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_transform_uses_segment_length_and_eye_width() {
        let transform = beam_transform(
            Vec2::new(1.0, 2.0),
            Vec2::new(4.0, 2.0),
            EyeBeamState {
                width: 0.25,
                active: true,
                ..default()
            },
        )
        .expect("horizontal beam is visible");
        assert_eq!(transform.translation.truncate(), Vec2::new(2.5, 2.0));
        assert_eq!(transform.scale, Vec3::new(3.0, 0.25, 1.0));
    }
}

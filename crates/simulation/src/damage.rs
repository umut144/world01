//! One message for every source of damage, and the only writer of health.

use bevy::prelude::*;
use world01_world_data::CharacterHealth;

/// What caused a point of damage.
///
/// Carried so that later consumers - a bot deciding whether to retaliate, a
/// hit indicator - can tell who hit them without the emitters telling them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSource {
    /// Another actor's attack.
    Actor(Entity),
    /// The actor's own exhaustion.
    Exhaustion,
}

/// A resolved hit, waiting to be applied to the target's health.
///
/// Emitters decide whether something was hit and how hard. `apply_damage`
/// decides what that does to health, and nothing else writes
/// [`CharacterHealth`].
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DamageDealt {
    pub target: Entity,
    pub source: DamageSource,
    pub amount: f32,
}

/// Applies every hit resolved this tick. The single writer of health.
pub fn apply_damage(
    mut dealt: MessageReader<DamageDealt>,
    mut targets: Query<&mut CharacterHealth>,
) {
    for damage in dealt.read() {
        if !damage.amount.is_finite() || damage.amount <= 0.0 {
            continue;
        }
        let Ok(mut health) = targets.get_mut(damage.target) else {
            continue;
        };
        health.current = (health.current - damage.amount).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_message::<DamageDealt>()
            .add_systems(Update, apply_damage);
        app
    }

    #[test]
    fn damage_subtracts_and_stops_at_zero() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(CharacterHealth::full(100.0)).id();

        app.world_mut().write_message(DamageDealt {
            target: actor,
            source: DamageSource::Exhaustion,
            amount: 30.0,
        });
        app.update();
        assert_eq!(
            app.world().get::<CharacterHealth>(actor).unwrap().current,
            70.0
        );

        app.world_mut().write_message(DamageDealt {
            target: actor,
            source: DamageSource::Exhaustion,
            amount: 1_000.0,
        });
        app.update();
        assert_eq!(
            app.world().get::<CharacterHealth>(actor).unwrap().current,
            0.0
        );
    }

    #[test]
    fn every_hit_in_a_tick_is_applied() {
        let mut app = test_app();
        let attacker = app.world_mut().spawn_empty().id();
        let actor = app.world_mut().spawn(CharacterHealth::full(100.0)).id();

        for amount in [10.0, 25.0] {
            app.world_mut().write_message(DamageDealt {
                target: actor,
                source: DamageSource::Actor(attacker),
                amount,
            });
        }
        app.update();

        assert_eq!(
            app.world().get::<CharacterHealth>(actor).unwrap().current,
            65.0
        );
    }

    #[test]
    fn a_hit_cannot_heal_and_a_missing_target_is_ignored() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(CharacterHealth::full(50.0)).id();
        let without_health = app.world_mut().spawn_empty().id();

        for message in [
            DamageDealt {
                target: actor,
                source: DamageSource::Exhaustion,
                amount: -20.0,
            },
            DamageDealt {
                target: actor,
                source: DamageSource::Exhaustion,
                amount: f32::NAN,
            },
            DamageDealt {
                target: without_health,
                source: DamageSource::Exhaustion,
                amount: 10.0,
            },
        ] {
            app.world_mut().write_message(message);
        }
        app.update();

        assert_eq!(
            app.world().get::<CharacterHealth>(actor).unwrap().current,
            50.0
        );
    }
}

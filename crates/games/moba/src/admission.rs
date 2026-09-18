//! Who joins a MOBA match, and where they appear.

use world01_simulation::{Admission, JoinRefused, JoinedIdentity, SessionRules};
use world01_world_data::{TeamId, WorldPosition};

use crate::{MobaWorldDerivation, TotemKind, TotemLayout};

impl SessionRules for MobaWorldDerivation {
    fn admit(brought: &JoinedIdentity, totems: &TotemLayout) -> Result<Admission, JoinRefused> {
        let Some(team) = brought.team else {
            return Err(JoinRefused::new(
                "the MOBA is played in sides and this join named none",
            ));
        };
        let Some(position) = totem_of_life_position(totems, team) else {
            return Err(JoinRefused::new(format!(
                "side {} has no Totem of Life to spawn behind",
                team.0
            )));
        };
        Ok(Admission {
            position,
            team: Some(team),
        })
    }
}

/// Where a team's Actors join the match: at their own Totem of Life.
///
/// Spawning on the objective rather than beside it is deliberate, not an
/// approximation waiting for an offset - the existing world-separation step
/// already resolves whatever overlap that causes, the same way it resolves
/// any other Character spawned inside a collider.
fn totem_of_life_position(totems: &TotemLayout, team: TeamId) -> Option<WorldPosition> {
    totems
        .totems
        .iter()
        .find(|placed| placed.totem.kind == TotemKind::Life && placed.totem.team == team)
        .map(|placed| placed.position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlacedTotem, Totem};
    use world01_world_data::CharacterId;

    fn two_sided_layout() -> TotemLayout {
        TotemLayout {
            totems: vec![
                PlacedTotem {
                    totem: Totem {
                        kind: TotemKind::Life,
                        team: TeamId(0),
                    },
                    position: WorldPosition::new(-10.0, 0.0, 1.0),
                    max_hp: 2000.0,
                },
                PlacedTotem {
                    totem: Totem {
                        kind: TotemKind::Mana,
                        team: TeamId(0),
                    },
                    position: WorldPosition::new(-8.0, 0.0, 1.0),
                    max_hp: 1000.0,
                },
                PlacedTotem {
                    totem: Totem {
                        kind: TotemKind::Life,
                        team: TeamId(1),
                    },
                    position: WorldPosition::new(10.0, 0.0, 1.0),
                    max_hp: 2000.0,
                },
            ],
        }
    }

    fn brought(team: Option<TeamId>) -> JoinedIdentity {
        JoinedIdentity {
            character: CharacterId::new("hammerer").expect("static character id"),
            team,
        }
    }

    #[test]
    fn a_team_spawns_at_its_own_totem_of_life() {
        let totems = two_sided_layout();

        assert_eq!(
            totem_of_life_position(&totems, TeamId(0)),
            Some(WorldPosition::new(-10.0, 0.0, 1.0))
        );
        assert_eq!(
            totem_of_life_position(&totems, TeamId(1)),
            Some(WorldPosition::new(10.0, 0.0, 1.0))
        );
    }

    #[test]
    fn an_admitted_player_enters_behind_the_totem_of_life_of_the_side_they_brought() {
        let admission = MobaWorldDerivation::admit(&brought(Some(TeamId(1))), &two_sided_layout())
            .expect("a side with a Totem of Life is admissible");

        assert_eq!(admission.position, WorldPosition::new(10.0, 0.0, 1.0));
        assert_eq!(admission.team, Some(TeamId(1)));
    }

    #[test]
    fn a_join_that_names_no_side_is_refused_rather_than_assigned_one() {
        let refusal = MobaWorldDerivation::admit(&brought(None), &two_sided_layout())
            .expect_err("a MOBA join without a side is not admissible");

        assert!(
            refusal.to_string().contains("named none"),
            "the refusal should say the side is missing, got: {refusal}"
        );
    }

    #[test]
    fn a_side_the_map_places_no_totem_of_life_for_is_refused() {
        let refusal = MobaWorldDerivation::admit(&brought(Some(TeamId(7))), &two_sided_layout())
            .expect_err("a side without a Totem of Life is not admissible");

        assert!(
            refusal.to_string().contains("no Totem of Life"),
            "the refusal should name the missing objective, got: {refusal}"
        );
    }
}

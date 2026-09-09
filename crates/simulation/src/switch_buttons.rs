//! Provisional switch buttons: a place on the map that throws a named switch
//! when somebody walks off it.
//!
//! Everything here is a placeholder for something SceneMaker will author. A
//! button is a Terrain cell painted with `cobblestone`, and which switch it
//! throws is written down below because it cannot be worked out from the map:
//! in `overworld01` both cobblestone cells lie nearest to the same body of
//! water, so distance answers the same for both and would be wrong for one.
//!
//! Registering on entering and throwing on leaving means a Character standing
//! on a button does nothing until it steps off, which needs no timer and
//! cannot flutter: one traversal is one throw.

use std::collections::HashMap;

use bevy::prelude::*;
use world01_world_data::{
    ActorId, CharacterLifeState, PlacementRanks, Position, WorldComposition, WorldMap,
    WorldPosition, WorldSwitchRequest, WorldTemplateCatalog,
};

use crate::world_runtime::WorldRuntimeSet;

/// The Terrain Asset a button is painted with while it has no Asset of its own.
const BUTTON_TERRAIN_ASSET: &str = "cobblestone";

/// Which switch the button on a given Terrain cell throws.
///
/// Authored by hand on purpose: the pairing is a decision, not a measurement.
/// A cobblestone cell that is not listed here is left alone and reported, and
/// an entry whose cell is not painted is reported too, so moving a tile shows
/// up as a message rather than as a button that quietly stopped working.
const AUTHORED_BUTTONS: &[(&str, u32, u32, &str)] = &[
    ("overworld01", 47, 39, "upper_valley"),
    ("overworld01", 73, 38, "branch_between_bridges"),
];

/// One button, at the middle of the cell it is painted on.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchButton {
    pub position: Position,
    pub elevation_meters: f32,
    pub switch: String,
}

/// The buttons of a map, in the order they are written down.
pub fn switch_buttons(map: &WorldMap) -> Vec<SwitchButton> {
    let mut buttons = Vec::new();
    for (scene_id, x, y, switch) in AUTHORED_BUTTONS {
        if *scene_id != map.scene_id() {
            continue;
        }
        let Some(cell) = map.terrain_cell(*x, *y) else {
            warn!(
                scene_id,
                x, y, switch, "no Terrain cell carries this button"
            );
            continue;
        };
        if cell.asset_key != BUTTON_TERRAIN_ASSET {
            warn!(
                scene_id,
                x,
                y,
                switch,
                asset_key = cell.asset_key.as_str(),
                "the cell this button names is painted with something else"
            );
            continue;
        }
        if map.switch_is_on(switch).is_none() {
            warn!(scene_id, x, y, switch, "this map declares no such switch");
            continue;
        }
        buttons.push(SwitchButton {
            position: cell.center,
            elevation_meters: cell.elevation_meters,
            switch: (*switch).to_owned(),
        });
    }
    buttons
}

/// Reports Terrain painted as a button that no switch is written down for.
///
/// Its own system, run once, because answering it means walking every Terrain
/// cell of the map and the answer cannot change between ticks.
pub fn report_unlisted_buttons(map: Res<WorldMap>) {
    for cell in map
        .terrain_cells()
        .iter()
        .filter(|cell| cell.asset_key == BUTTON_TERRAIN_ASSET)
    {
        if !AUTHORED_BUTTONS
            .iter()
            .any(|(scene_id, x, y, _)| *scene_id == map.scene_id() && *x == cell.x && *y == cell.y)
        {
            warn!(
                scene_id = map.scene_id(),
                x = cell.x,
                y = cell.y,
                "a button is painted here that no switch is written down for"
            );
        }
    }
}

/// Whether a point is on a button, which is the Terrain cell it is painted on.
fn stands_on(button: &SwitchButton, at: Position, cell_meters: f32) -> bool {
    let half = cell_meters / 2.0;
    (at.x - button.position.x).abs() <= half && (at.y - button.position.y).abs() <= half
}

/// Which button each Actor is standing on, so that stepping off can be seen.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ButtonOccupants {
    standing_on: HashMap<u64, usize>,
}

/// Throws a switch when a Character walks off the button that names it.
///
/// Server-owned: the positions this reads are the authoritative ones, and the
/// result is submitted the way an Anchor change is, so the whole derived world
/// is validated before anything is committed and every client is told
/// afterwards.
pub fn throw_switches_walked_off(
    map: Res<WorldMap>,
    composition: Res<WorldComposition>,
    templates: Res<WorldTemplateCatalog>,
    ranks: Res<PlacementRanks>,
    mut occupants: ResMut<ButtonOccupants>,
    mut request: ResMut<WorldSwitchRequest>,
    actors: Query<(&ActorId, &WorldPosition, &CharacterLifeState)>,
) {
    let buttons = switch_buttons(&map);
    if buttons.is_empty() {
        return;
    }
    let cell_meters = map.terrain_cell_meters();

    let mut left = Vec::new();
    let mut standing_now = HashMap::new();
    for (actor, position, life) in &actors {
        // A Character that is not alive is not walking anywhere: dying on a
        // button must not throw it.
        if *life != CharacterLifeState::Alive {
            standing_now.extend(occupants.standing_on.get(&actor.0).map(|on| (actor.0, *on)));
            continue;
        }
        let on = buttons
            .iter()
            .position(|button| stands_on(button, position.horizontal(), cell_meters));
        let was = occupants.standing_on.get(&actor.0).copied();
        if let Some(was) = was
            && was != on.unwrap_or(usize::MAX)
        {
            left.push(was);
        }
        if let Some(on) = on {
            standing_now.insert(actor.0, on);
        }
    }
    occupants.standing_on = standing_now;

    for index in left {
        let Some(button) = buttons.get(index) else {
            continue;
        };
        let Some(on) = map.switch_is_on(&button.switch) else {
            continue;
        };
        let mut candidate = composition.clone();
        match candidate.set_switch(&button.switch, !on, &templates, &ranks) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                warn!(%error, switch = button.switch.as_str(), "cannot throw this switch");
                continue;
            }
        }
        let offered = request
            .latest()
            .map_or(composition.switches().generation(), |switches| {
                switches.generation()
            });
        if let Err(error) = candidate.ensure_switch_generation_newer_than(offered) {
            warn!(%error, "cannot advance the switch generation");
            continue;
        }
        info!(
            target: "game_console",
            "switch '{}' thrown to {}",
            button.switch,
            if on { "off" } else { "on" },
        );
        request.submit(candidate.switches().clone());
    }
}

/// Installs the provisional buttons on the authoritative side.
pub fn add_switch_buttons(app: &mut App, schedule: impl bevy::ecs::schedule::ScheduleLabel) {
    app.init_resource::<ButtonOccupants>()
        .add_systems(Startup, report_unlisted_buttons)
        .add_systems(
            schedule,
            throw_switches_walked_off.before(WorldRuntimeSet::Rebuild),
        );
}

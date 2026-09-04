use std::collections::BTreeMap;

use bevy::prelude::{Component, Resource};
use serde::{Deserialize, Serialize};

use crate::{PlacementRanks, WorldMap, WorldMapError, WorldTemplateCatalog};

/// The server-decided Template occupant of each non-empty Anchor.
///
/// Absence is the normal empty-Anchor state. The generation changes only when
/// an occupant actually changes and gives replication a compact version key.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorOccupancy {
    generation: u64,
    occupants: BTreeMap<String, String>,
}

/// The newest authority snapshot offered to the local world runtime.
///
/// Transport and event-selection code submit snapshots here. The simulation
/// validates the complete derived world before committing a newer generation,
/// so receiving a snapshot alone never creates a partially updated world.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldOccupancyRequest {
    latest: Option<AnchorOccupancy>,
}

impl WorldOccupancyRequest {
    pub fn latest(&self) -> Option<&AnchorOccupancy> {
        self.latest.as_ref()
    }

    /// Retains only the newest submitted generation.
    pub fn submit(&mut self, occupancy: AnchorOccupancy) -> bool {
        if self
            .latest
            .as_ref()
            .is_some_and(|latest| latest.generation() >= occupancy.generation())
        {
            return false;
        }
        self.latest = Some(occupancy);
        true
    }
}

impl AnchorOccupancy {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn occupant(&self, anchor_id: &str) -> Option<&str> {
        self.occupants.get(anchor_id).map(String::as_str)
    }

    pub fn occupants(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.occupants
            .iter()
            .map(|(anchor_id, template_scene_id)| (anchor_id.as_str(), template_scene_id.as_str()))
    }

    fn advance_generation(&mut self) -> Result<(), WorldMapError> {
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            WorldMapError::new("Anchor occupancy generation cannot advance beyond u64::MAX")
        })?;
        Ok(())
    }
}

/// An immutable authored base map plus its current derived composition.
///
/// Every occupancy change recomputes from `base_map` and folds occupied
/// Anchors in their authored order. It never attempts to undo an earlier merge.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct WorldComposition {
    base_map: WorldMap,
    occupancy: AnchorOccupancy,
    current_map: WorldMap,
}

impl WorldComposition {
    pub fn new(
        base_map: WorldMap,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<Self, WorldMapError> {
        Self::from_occupancy(base_map, AnchorOccupancy::default(), templates, ranks)
    }

    pub fn from_occupancy(
        base_map: WorldMap,
        occupancy: AnchorOccupancy,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<Self, WorldMapError> {
        let current_map = compose_map(&base_map, &occupancy, templates, ranks)?;
        Ok(Self {
            base_map,
            occupancy,
            current_map,
        })
    }

    pub fn base_map(&self) -> &WorldMap {
        &self.base_map
    }

    pub fn occupancy(&self) -> &AnchorOccupancy {
        &self.occupancy
    }

    pub fn current_map(&self) -> &WorldMap {
        &self.current_map
    }

    /// Ensures an authority-created snapshot supersedes every generation that
    /// has already been offered to the runtime, including a rejected one.
    pub fn ensure_generation_newer_than(&mut self, generation: u64) -> Result<(), WorldMapError> {
        if self.occupancy.generation <= generation {
            self.occupancy.generation = generation.checked_add(1).ok_or_else(|| {
                WorldMapError::new("Anchor occupancy generation cannot advance beyond u64::MAX")
            })?;
        }
        Ok(())
    }

    /// Mutates this protocol-neutral value directly.
    ///
    /// Runtime code must clone the installed composition, mutate the clone,
    /// and submit its occupancy through [`WorldOccupancyRequest`]. Directly
    /// mutating the installed resource bypasses atomic derived-world
    /// validation.
    pub fn set_occupant(
        &mut self,
        anchor_id: &str,
        template_scene_id: &str,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<bool, WorldMapError> {
        if self.occupancy.occupant(anchor_id) == Some(template_scene_id) {
            return Ok(false);
        }

        let mut occupancy = self.occupancy.clone();
        occupancy
            .occupants
            .insert(anchor_id.to_owned(), template_scene_id.to_owned());
        occupancy.advance_generation()?;
        self.replace_occupancy(occupancy, templates, ranks)?;
        Ok(true)
    }

    /// Mutates this protocol-neutral value directly.
    ///
    /// Runtime code must clone the installed composition, mutate the clone,
    /// and submit its occupancy through [`WorldOccupancyRequest`]. Directly
    /// mutating the installed resource bypasses atomic derived-world
    /// validation.
    pub fn clear_occupant(
        &mut self,
        anchor_id: &str,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<bool, WorldMapError> {
        if !self
            .base_map
            .template_anchors()
            .iter()
            .any(|anchor| anchor.anchor_id == anchor_id)
        {
            return Err(WorldMapError::new(format!(
                "Template Anchor '{anchor_id}' does not exist in Instance '{}'",
                self.base_map.scene_id()
            )));
        }
        if self.occupancy.occupant(anchor_id).is_none() {
            return Ok(false);
        }

        let mut occupancy = self.occupancy.clone();
        occupancy.occupants.remove(anchor_id);
        occupancy.advance_generation()?;
        self.replace_occupancy(occupancy, templates, ranks)?;
        Ok(true)
    }

    /// Applies a replicated authority snapshot only when its generation is
    /// newer than the one already derived locally.
    ///
    /// Equal and older snapshots are harmless no-ops. Invalid newer snapshots
    /// leave both the accepted occupancy and current map unchanged.
    pub fn apply_newer_occupancy(
        &mut self,
        occupancy: AnchorOccupancy,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<bool, WorldMapError> {
        if occupancy.generation <= self.occupancy.generation {
            return Ok(false);
        }

        self.replace_occupancy(occupancy, templates, ranks)?;
        Ok(true)
    }

    fn replace_occupancy(
        &mut self,
        occupancy: AnchorOccupancy,
        templates: &WorldTemplateCatalog,
        ranks: &PlacementRanks,
    ) -> Result<(), WorldMapError> {
        let current_map = compose_map(&self.base_map, &occupancy, templates, ranks)?;
        self.occupancy = occupancy;
        self.current_map = current_map;
        Ok(())
    }
}

fn compose_map(
    base_map: &WorldMap,
    occupancy: &AnchorOccupancy,
    templates: &WorldTemplateCatalog,
    ranks: &PlacementRanks,
) -> Result<WorldMap, WorldMapError> {
    ranks.validate_for(base_map, templates)?;

    let mut placements = BTreeMap::new();
    for (anchor_id, template_scene_id) in occupancy.occupants() {
        let template = templates.template(template_scene_id).ok_or_else(|| {
            WorldMapError::new(format!(
                "Template '{template_scene_id}' does not exist for Anchor '{anchor_id}'"
            ))
        })?;
        let placement = base_map.project_template(anchor_id, template)?;
        placements.insert(anchor_id, placement);
    }

    let mut current_map = base_map.clone();
    for anchor in base_map.template_anchors() {
        if let Some(placement) = placements.get(anchor.anchor_id.as_str()) {
            current_map = current_map.merged_with(placement, ranks)?;
        }
    }
    Ok(current_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldTemplate, map::template_export, map::test_export_with_anchors};

    const FIRST_ANCHOR: &str = r#"{
        "anchor_id": "zeta_anchor",
        "group_number": 1,
        "position_authoring_px": { "x": 64, "y": 96 }
    }"#;
    const SECOND_ANCHOR: &str = r#"{
        "anchor_id": "alpha_anchor",
        "group_number": 1,
        "position_authoring_px": { "x": 64, "y": 96 }
    }"#;
    const BASE_CELL: &str = r#"{ "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": 1.0 }"#;

    fn template(scene_id: &str, group_number: u32, elevation_meters: f32) -> WorldTemplate {
        template_with_props(scene_id, group_number, elevation_meters, "")
    }

    fn template_with_props(
        scene_id: &str,
        group_number: u32,
        elevation_meters: f32,
        props: &str,
    ) -> WorldTemplate {
        let definition = format!(
            r#"{{
                "group_number": {group_number},
                "insertion_anchor_authoring_px": {{ "x": 64, "y": 96 }}
            }}"#
        );
        let cell = format!(
            r#"{{ "x": 0, "y": 0, "asset_key": "grass", "elevation_meters": {elevation_meters} }}"#
        );
        let source = template_export(&definition, &cell, props).replace(
            r#""scene_id": "test_template_unit""#,
            &format!(r#""scene_id": "{scene_id}""#),
        );
        WorldTemplate::from_source(&source, scene_id).expect("the synthetic Template is valid")
    }

    fn fixture() -> (WorldMap, WorldTemplateCatalog, PlacementRanks) {
        let anchors = format!("{FIRST_ANCHOR}, {SECOND_ANCHOR}");
        let map = WorldMap::from_source(
            &test_export_with_anchors(BASE_CELL, "", &anchors),
            "overworld01",
        )
        .expect("the synthetic Instance is valid");
        let templates = WorldTemplateCatalog::from_templates([
            template("template_first", 1, 2.0),
            template("template_second", 1, 3.0),
            template("template_wrong_group", 2, 4.0),
        ]);
        let ranks = PlacementRanks::from_entries([("grass", 10)])
            .expect("the synthetic Placement Ranks are valid");
        (map, templates, ranks)
    }

    #[test]
    fn empty_occupancy_is_the_unchanged_base_map() {
        let (map, templates, ranks) = fixture();
        let composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("an empty composition is valid");

        assert_eq!(composition.base_map(), &map);
        assert_eq!(composition.current_map(), &map);
        assert_eq!(composition.occupancy().generation(), 0);
        assert_eq!(composition.occupancy().occupants().len(), 0);
    }

    #[test]
    fn authored_anchor_order_is_independent_from_assignment_order() {
        let (map, templates, ranks) = fixture();
        let mut forward = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the empty composition is valid");
        let mut reverse =
            WorldComposition::new(map, &templates, &ranks).expect("the empty composition is valid");

        forward
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the first assignment is valid");
        forward
            .set_occupant("alpha_anchor", "template_second", &templates, &ranks)
            .expect("the second assignment is valid");
        reverse
            .set_occupant("alpha_anchor", "template_second", &templates, &ranks)
            .expect("the second assignment is valid");
        reverse
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the first assignment is valid");

        assert_eq!(forward.current_map(), reverse.current_map());
        assert_eq!(forward.occupancy(), reverse.occupancy());
        assert_eq!(
            forward.current_map().terrain_cells()[0].elevation_meters,
            3.0
        );
        assert_eq!(forward.occupancy().generation(), 2);
    }

    #[test]
    fn occupancy_changes_recompose_from_base_and_advance_once() {
        let (map, templates, ranks) = fixture();
        let mut composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the empty composition is valid");

        assert!(
            composition
                .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
                .expect("the assignment is valid")
        );
        assert!(
            !composition
                .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
                .expect("the identical assignment is a no-op")
        );
        assert_eq!(composition.occupancy().generation(), 1);
        assert!(
            composition
                .clear_occupant("zeta_anchor", &templates, &ranks)
                .expect("clearing the assignment is valid")
        );
        assert_eq!(composition.current_map(), &map);
        assert_eq!(composition.occupancy().generation(), 2);
        assert!(
            !composition
                .clear_occupant("zeta_anchor", &templates, &ranks)
                .expect("clearing an empty Anchor is a no-op")
        );
        assert_eq!(composition.occupancy().generation(), 2);
    }

    #[test]
    fn authority_can_supersede_an_already_offered_generation() {
        let (map, templates, ranks) = fixture();
        let mut composition =
            WorldComposition::new(map, &templates, &ranks).expect("the composition is valid");
        composition
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the assignment is valid");

        composition
            .ensure_generation_newer_than(5)
            .expect("the generation can advance past the offered snapshot");

        assert_eq!(composition.occupancy().generation(), 6);
        assert_eq!(
            composition.occupancy().occupant("zeta_anchor"),
            Some("template_first")
        );
    }

    #[test]
    fn replicated_occupancy_only_applies_newer_generations() {
        let (map, templates, ranks) = fixture();
        let mut authority = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the authority starts empty");
        authority
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the first authority change is valid");
        let first_snapshot = authority.occupancy().clone();
        let first_map = authority.current_map().clone();
        authority
            .set_occupant("alpha_anchor", "template_second", &templates, &ranks)
            .expect("the second authority change is valid");
        let second_snapshot = authority.occupancy().clone();

        let mut replica =
            WorldComposition::new(map, &templates, &ranks).expect("the replica starts empty");
        assert!(
            replica
                .apply_newer_occupancy(first_snapshot.clone(), &templates, &ranks)
                .expect("the first snapshot is valid")
        );
        assert_eq!(replica.current_map(), &first_map);
        assert!(
            !replica
                .apply_newer_occupancy(first_snapshot, &templates, &ranks)
                .expect("a repeated snapshot is a no-op")
        );
        assert!(
            !replica
                .apply_newer_occupancy(AnchorOccupancy::default(), &templates, &ranks)
                .expect("an older snapshot is a no-op")
        );
        assert!(
            replica
                .apply_newer_occupancy(second_snapshot, &templates, &ranks)
                .expect("the newer snapshot is valid")
        );
        assert_eq!(replica.current_map(), authority.current_map());
        assert_eq!(replica.occupancy(), authority.occupancy());

        let accepted = replica.clone();
        let invalid_snapshot = AnchorOccupancy {
            generation: 3,
            occupants: BTreeMap::from([("missing_anchor".to_owned(), "template_first".to_owned())]),
        };
        assert!(
            replica
                .apply_newer_occupancy(invalid_snapshot, &templates, &ranks)
                .is_err()
        );
        assert_eq!(replica, accepted);
    }

    #[test]
    fn invalid_occupants_are_rejected_without_changing_the_composition() {
        let (map, templates, ranks) = fixture();
        let mut composition =
            WorldComposition::new(map, &templates, &ranks).expect("the empty composition is valid");
        let original = composition.clone();

        assert!(
            composition
                .set_occupant("missing", "template_first", &templates, &ranks)
                .is_err()
        );
        assert!(
            composition
                .set_occupant("zeta_anchor", "missing", &templates, &ranks)
                .is_err()
        );
        assert!(
            composition
                .set_occupant("zeta_anchor", "template_wrong_group", &templates, &ranks,)
                .is_err()
        );
        assert!(
            composition
                .clear_occupant("missing", &templates, &ranks)
                .is_err()
        );
        assert_eq!(composition, original);
    }

    #[test]
    fn occupancy_has_a_stable_serialized_form() {
        let (map, templates, ranks) = fixture();
        let mut composition = WorldComposition::new(map.clone(), &templates, &ranks)
            .expect("the empty composition is valid");
        composition
            .set_occupant("alpha_anchor", "template_second", &templates, &ranks)
            .expect("the assignment is valid");
        composition
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the assignment is valid");

        let encoded = serde_json::to_string(composition.occupancy())
            .expect("occupancy serializes for replication");
        let decoded: AnchorOccupancy =
            serde_json::from_str(&encoded).expect("occupancy deserializes for replication");

        assert_eq!(
            encoded,
            r#"{"generation":2,"occupants":{"alpha_anchor":"template_second","zeta_anchor":"template_first"}}"#
        );
        assert_eq!(decoded, *composition.occupancy());
        let reconstructed = WorldComposition::from_occupancy(map, decoded, &templates, &ranks)
            .expect("the replicated occupancy reconstructs the composition");
        assert_eq!(reconstructed.current_map(), composition.current_map());
    }

    #[test]
    fn later_anchor_terrain_can_remove_an_earlier_template_prop() {
        let anchors = format!("{FIRST_ANCHOR}, {SECOND_ANCHOR}");
        let map = WorldMap::from_source(
            &test_export_with_anchors(BASE_CELL, "", &anchors),
            "overworld01",
        )
        .expect("the synthetic Instance is valid");
        let earlier_prop = r#"{
            "instance_id": "temporary_tree_prop",
            "asset_key": "tree",
            "position_authoring_px": { "x": 16, "y": 16 },
            "elevation_meters": 1.0
        }"#;
        let templates = WorldTemplateCatalog::from_templates([
            template_with_props("template_first", 1, 2.0, earlier_prop),
            template("template_second", 1, 3.0),
        ]);
        let ranks = PlacementRanks::from_entries([("grass", 10), ("tree", 10)])
            .expect("the synthetic Placement Ranks are valid");
        let mut composition =
            WorldComposition::new(map, &templates, &ranks).expect("the empty composition is valid");

        composition
            .set_occupant("zeta_anchor", "template_first", &templates, &ranks)
            .expect("the earlier assignment is valid");
        assert_eq!(composition.current_map().props().len(), 1);

        composition
            .set_occupant("alpha_anchor", "template_second", &templates, &ranks)
            .expect("the later assignment is valid");
        assert!(composition.current_map().props().is_empty());
    }
}

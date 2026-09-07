#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_directory="${SCENEMAKER_EXPORTS:-$project_root/../../GodotProjects/SceneMaker/workspaces/world01/exports}"
asset_catalog="$project_root/assets/catalog.json"
destination_directory="$project_root/assets/maps"
staging_directory=""
backup_directory=""

fail() {
  printf 'SCENEMAKER SYNC FAILED: %s\n' "$1" >&2
  exit 1
}

cleanup() {
  if [[ -n "$staging_directory" && -d "$staging_directory" ]]; then
    rm -rf -- "$staging_directory"
  fi
  if [[ -n "$backup_directory" && -d "$backup_directory" ]]; then
    if [[ ! -e "$destination_directory" ]]; then
      mv "$backup_directory" "$destination_directory"
    else
      rm -rf -- "$backup_directory"
    fi
  fi
}
trap cleanup EXIT

command -v jq >/dev/null 2>&1 || fail 'jq is required.'
[[ -d "$source_directory" ]] || fail "export directory not found: $source_directory"
[[ -f "$asset_catalog" ]] || fail "world01 asset catalog not found: $asset_catalog"

# The Keys that only exist as members of a Palette. A map names the Palette, so
# that surface and Placement Rank attach to the choice and not to one of the
# Assets it chooses between.
palette_variants=""
while IFS= read -r asset_manifest; do
  variants="$(jq -r '
    select(.asset_category == "palette")
    | .asset_type as $asset_type
    | .variants[]
    | [., $asset_type]
    | @tsv
  ' "$asset_manifest")" || fail "cannot read asset manifest: $asset_manifest"
  [[ -z "$variants" ]] || palette_variants+="$variants"$'\n'
done < <(find "$project_root/assets" -type f -name 'manifest.json' | LC_ALL=C sort)

source_exports=()
while IFS= read -r source_export; do
  source_exports+=("$source_export")
done < <(find "$source_directory" -maxdepth 1 -type f -name '*.scene_export.json' -print | LC_ALL=C sort)
[[ ${#source_exports[@]} -gt 0 ]] || fail "no SceneMaker exports found in: $source_directory"

staging_directory="$(mktemp -d "$project_root/assets/.maps.XXXXXX")"
for source_export in "${source_exports[@]}"; do
  jq -e '
    def grid_anchor($position; $width; $height; $step):
      ($position.x | type == "number" and . == floor and . >= 0 and . <= ($width * $step)
        and ((. / $step) == ((. / $step) | floor)))
      and ($position.y | type == "number" and . == floor and . >= 0 and . <= ($height * $step)
        and ((. / $step) == ((. / $step) | floor)));
    def route_operation($segment):
      ($segment.operation == "additive" and $segment.clearance_above_meters == null)
      or ($segment.operation == "subtractive"
        and ($segment.clearance_above_meters | type == "number" and isfinite and . > 0));
    def bridge_ground($ground):
      ($ground == null)
      or (($ground.elevation_meters | type == "number" and isfinite)
        and ($ground.asset_key | type == "string" and length > 0)
        and ($ground.source_id | . == null or (type == "string" and length > 0)));
    def scene_position($position; $width; $height; $step):
      ($position.x | type == "number" and . == floor and . >= 0 and . <= ($width * $step))
      and ($position.y | type == "number" and . == floor and . >= 0 and . <= ($height * $step));

    . as $root
    | ($root.grid.terrain_cell_meters * $root.grid.authoring_pixels_per_meter) as $terrain_step
    | ($root.grid.terrain_cell_meters / $root.grid.water_cell_meters) as $water_cells
    | .format == "scene_maker_scene_export"
    and .version == 16
    and .workspace_key == "world01"
    and (.grid.terrain_cell_meters | type == "number" and isfinite and . > 0)
    and (.grid.authoring_pixels_per_meter | type == "number" and isfinite and . > 0)
    and (.grid.game_pixels_per_meter | type == "number" and isfinite and . > 0)
    and (.grid.water_cell_meters | type == "number" and isfinite and . > 0)
    and .scene.schema == "srt.scene_maker_scene"
    and .scene.version == 15
    and (.scene.scene_id | type == "string" and length > 0)
    and (.scene.scene_kind == "instance" or .scene.scene_kind == "template")
    and .scene.coordinate_space == "scene_local_bottom_left_y_up"
    and (.scene.size_cells.width | type == "number" and . == floor and . > 0)
    and (.scene.size_cells.height | type == "number" and . == floor and . > 0)
    and (.scene.terrain_cells | type == "array")
    and (.scene.props | type == "array")
    and (.scene.template_anchors | type == "array")
    and (.scene.water_bodies | type == "array")
    and (.water_raster | type == "array")
    and (.scene.route_surfaces | type == "array")
    and (.route_surface_bakes | type == "array")
    and (.route_surface_cut_raster | type == "array")
    and (.scene.bridges | type == "array")
    and (.bridge_bakes | type == "array")
    and (.water_bakes | type == "array")
    and (if .scene.scene_kind == "instance" then
      .scene.template_definition == null
    else
      (.scene.template_definition | type == "object")
      and (.scene.template_definition.group_number | type == "number" and . == floor and . > 0)
      and grid_anchor(.scene.template_definition.insertion_anchor_authoring_px;
        .scene.size_cells.width; .scene.size_cells.height; $terrain_step)
      and (.scene.template_anchors | length == 0)
      and (.scene.water_bodies | length == 0)
      and (.water_raster | length == 0)
      and (.scene.route_surfaces | length == 0)
      and (.route_surface_bakes | length == 0)
      and (.route_surface_cut_raster | length == 0)
      and (.scene.bridges | length == 0)
      and (.bridge_bakes | length == 0)
      and (.water_bakes | length == 0)
    end)
    and (.asset_profiles | type == "array")
    and (([.asset_profiles[].asset_key] | unique | length) == (.asset_profiles | length))
    and ([.asset_profiles[] | select(.surface != null) | .asset_key] | length) > 0
    and all(.scene.terrain_cells[];
      (.x | type == "number" and . == floor and . >= 0 and . < $root.scene.size_cells.width)
      and (.y | type == "number" and . == floor and . >= 0 and . < $root.scene.size_cells.height)
      and (.asset_key | type == "string" and length > 0)
      and (.elevation_meters | type == "number" and isfinite))
    and (([.scene.terrain_cells[] | "\(.x):\(.y)"] | unique | length) == (.scene.terrain_cells | length))
    and (([.scene.terrain_cells[].asset_key] | unique) - [.asset_profiles[] | select(.surface != null) | .asset_key] | length) == 0
    and all(.scene.props[];
      (.instance_id | type == "string" and length > 0)
      and (.asset_key | type == "string" and length > 0)
      and (.position_authoring_px.x | type == "number" and . == floor)
      and (.position_authoring_px.y | type == "number" and . == floor)
      and (if $root.scene.scene_kind == "instance" then
        scene_position(.position_authoring_px; $root.scene.size_cells.width;
          $root.scene.size_cells.height; $terrain_step)
      else true end)
      and (.elevation_meters | type == "number" and isfinite)
      and (. as $prop
        | [$root.asset_profiles[] | select(.asset_key == $prop.asset_key)] as $profiles
        | ($profiles | length) == 1
        and ($profiles[0] as $profile
          | ($profile.footprint_meters | type == "object")
          and ($profile.footprint_meters.width | type == "number" and isfinite and . > 0)
          and ($profile.footprint_meters.height | type == "number" and isfinite and . > 0)
          and ($profile.anchor_meters | type == "object")
          and ($profile.anchor_meters.x | type == "number" and isfinite and . >= 0
            and . <= $profile.footprint_meters.width)
          and ($profile.anchor_meters.y | type == "number" and isfinite and . >= 0
            and . <= $profile.footprint_meters.height))))
    and (([.scene.props[].instance_id] | unique | length) == (.scene.props | length))
    and all(.scene.template_anchors[];
      (.anchor_id | type == "string" and length > 0)
      and (.group_number | type == "number" and . == floor and . > 0)
      and grid_anchor(.position_authoring_px; $root.scene.size_cells.width;
        $root.scene.size_cells.height; $terrain_step))
    and (([.scene.template_anchors[].anchor_id] | unique | length)
      == (.scene.template_anchors | length))
    and (([.scene.route_surfaces[].route_surface_id] | unique | length)
      == (.scene.route_surfaces | length))
    and (([.route_surface_bakes[].route_surface_id] | unique | length)
      == (.route_surface_bakes | length))
    and ([.scene.route_surfaces[].route_surface_id]
      == [.route_surface_bakes[].route_surface_id])
    and all(.scene.route_surfaces[]; . as $route
      | (.route_surface_id | type == "string" and length > 0)
      and (.asset_key | type == "string" and length > 0)
      and (.points | type == "array" and length >= 2)
      and (.segments | type == "array" and length == (($route.points | length) - 1))
      and all($route.points[];
        (.position_authoring_px.x | type == "number" and . == floor)
        and (.position_authoring_px.y | type == "number" and . == floor)
        and (.elevation_meters | type == "number" and isfinite)
        and (.width_meters | type == "number" and isfinite and . > 0))
      and all($route.segments[];
        (.segment_id | type == "string" and length > 0)
        and (.grade_percent == -50 or .grade_percent == -25
          or .grade_percent == 0 or .grade_percent == 25 or .grade_percent == 50)
        and route_operation(.)))
    and all(.route_surface_bakes[];
      (.vertices | type == "array" and length > 0)
      and (.triangle_indices | type == "array" and length > 0
        and ((length % 3) == 0))
      and (.boundary_edges | type == "array" and length > 0)
      and (.centerline_samples | type == "array" and length >= 2)
      and (.segments | type == "array" and length >= 1)
      and all(.segments[]; route_operation(.))
      and all(.vertices[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite))
      and all(.centerline_samples[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite)
        and (.width_meters | type == "number" and isfinite and . > 0)
        and (.station_meters | type == "number" and isfinite and . >= 0)))
    and (([.scene.bridges[].bridge_id] | unique | length) == (.scene.bridges | length))
    and ([.scene.bridges[].bridge_id] == [.bridge_bakes[].bridge_id])
    and all(.scene.bridges[];
      (.bridge_id | type == "string" and length > 0)
      and (.plank_asset_key | type == "string" and length > 0)
      and (.anchor_asset_key | type == "string" and length > 0)
      and scene_position(.start_authoring_px; $root.scene.size_cells.width;
        $root.scene.size_cells.height; $terrain_step)
      and scene_position(.end_authoring_px; $root.scene.size_cells.width;
        $root.scene.size_cells.height; $terrain_step)
      and (.width_meters | type == "number" and isfinite and . > 0)
      and (.elevation_meters | type == "number" and isfinite)
      and (.plank_count | type == "number" and . == floor and . > 0)
      and (.plank_gap_meters | type == "number" and isfinite and . >= 0))
    and all(.bridge_bakes[]; . as $bake
      | ([$root.scene.bridges[] | select(.bridge_id == $bake.bridge_id)]) as $authored
      | ($authored | length) == 1
      and ($authored[0] as $bridge
        | ($bake.plank_asset_key == $bridge.plank_asset_key)
        and ($bake.plank_count == $bridge.plank_count)
        and ($bake.plank_gap_meters == $bridge.plank_gap_meters))
      and (.length_meters | type == "number" and isfinite and . > 0)
      and (.heading_degrees | type == "number" and isfinite)
      and (.centerline_samples | type == "array" and length >= 2)
      and ([.centerline_samples[].station_meters] == ([.centerline_samples[].station_meters] | sort))
      and (.centerline_samples[0].station_meters == 0)
      and (.centerline_samples[-1].station_meters == $bake.length_meters)
      and all(.centerline_samples[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite)
        and (.width_meters | type == "number" and isfinite and . > 0)
        and (.station_meters | type == "number" and isfinite and . >= 0))
      and bridge_ground(.ground_at_start)
      and bridge_ground(.ground_at_end)
      and (.plank_depth_meters | type == "number" and isfinite and . > 0)
      and (.vertices | type == "array" and length == 4)
      and all(.vertices[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite))
      and (.triangle_indices | type == "array" and length > 0 and ((length % 3) == 0))
      and all(.triangle_indices[];
        type == "number" and . == floor and . >= 0 and . < ($bake.vertices | length))
      and (.boundary_edges | type == "array" and length == 4)
      and all(.boundary_edges[];
        (.start_vertex_index | type == "number" and . == floor and . >= 0
          and . < ($bake.vertices | length))
        and (.end_vertex_index | type == "number" and . == floor and . >= 0
          and . < ($bake.vertices | length)))
      and (.planks | type == "array" and length == $bake.plank_count)
      and (([.planks[].plank_id] | unique | length) == (.planks | length))
      and all(.planks[];
        (.plank_id | type == "string" and length > 0)
        and (.asset_key == $bake.plank_asset_key)
        and (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite)
        and (.depth_meters | type == "number" and isfinite and . > 0)
        and (.width_meters | type == "number" and isfinite and . > 0))
      and (.posts | type == "array" and length == 4)
      and (([.posts[].post_id] | unique | length) == 4)
      and (([.posts[].corner] | unique | length) == 4)
      and all(.posts[]; . as $post
        | ($post.post_id | type == "string" and length > 0)
        and ($post.corner | type == "string" and length > 0)
        and ($post.asset_key == ($root.scene.bridges[]
          | select(.bridge_id == $bake.bridge_id) | .anchor_asset_key))
        and ($post.x_meters | type == "number" and isfinite)
        and ($post.y_meters | type == "number" and isfinite)
        and ($post.elevation_meters | type == "number" and isfinite)))
    and (([.route_surface_cut_raster[].route_surface_id] | unique | length)
      == (.route_surface_cut_raster | length))
    and (([.route_surface_cut_raster[].route_surface_id] | sort)
      == ([.scene.route_surfaces[]
        | select(any(.segments[]; .operation == "subtractive"))
        | .route_surface_id] | sort))
    and ($water_cells == ($water_cells | floor) and $water_cells >= 1)
    and (([.scene.water_bodies[].water_body_id] | unique | length)
      == (.scene.water_bodies | length))
    and ([.scene.water_bodies[].water_body_id] == [.water_raster[].water_body_id])
    and all(.scene.water_bodies[];
      (.water_body_id | type == "string" and length > 0)
      and (.water_kind | type == "string" and length > 0)
      and (.asset_key | type == "string" and length > 0)
      and (.points | type == "array" and length >= 2)
      and all(.points[];
        (.position_authoring_px.x | type == "number" and . == floor)
        and (.position_authoring_px.y | type == "number" and . == floor)
        and (.elevation_meters | type == "number" and isfinite)
        and (.width_meters | type == "number" and isfinite and . > 0)
        and (.channel_depth_meters | type == "number" and isfinite and . > 0)
        and (.clearance_above_meters | type == "number" and isfinite and . >= 0)))
    and ([.scene.water_bodies[].water_body_id] == [.water_bakes[].water_body_id])
    and all(.water_bakes[]; . as $bake
      | (.asset_key | type == "string" and length > 0)
      and (.vertices | type == "array" and length >= 3)
      and all(.vertices[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite))
      and (.triangle_indices | type == "array" and length > 0 and ((length % 3) == 0))
      and all(.triangle_indices[];
        type == "number" and . == floor and . >= 0 and . < ($bake.vertices | length))
      and (.centerline_samples | type == "array" and length >= 2)
      and (.centerline_samples[0].station_meters == 0)
      and ([.centerline_samples[].station_meters]
        == ([.centerline_samples[].station_meters] | sort))
      and all(.centerline_samples[];
        (.x_meters | type == "number" and isfinite)
        and (.y_meters | type == "number" and isfinite)
        and (.elevation_meters | type == "number" and isfinite)
        and (.width_meters | type == "number" and isfinite and . > 0)
        and (.station_meters | type == "number" and isfinite and . >= 0)))
    and all(.water_raster[];
      (.cells | type == "array" and length > 0)
      and all(.cells[];
        (.x | type == "number" and . == floor and . >= 0
          and . < ($root.scene.size_cells.width * $water_cells))
        and (.y | type == "number" and . == floor and . >= 0
          and . < ($root.scene.size_cells.height * $water_cells))
        and (.bed_meters | type == "number" and isfinite)
        and (.surface_meters | type == "number" and isfinite)
        and (.cut_top_meters | type == "number" and isfinite)
        and (.surface_meters > .bed_meters)
        and (.cut_top_meters >= .surface_meters))
      and (([.cells[] | "\(.x):\(.y)"] | unique | length) == (.cells | length)))
    and all(.route_surface_cut_raster[]; . as $cut
      | ([$root.scene.route_surfaces[]
          | select(.route_surface_id == $cut.route_surface_id)
          | .segments[]
          | select(.operation == "subtractive")
          | .segment_id]) as $excavating
      | all($cut.cells[]; . as $cell
        | ($cell.x | type == "number" and . == floor and . >= 0
          and . < ($root.scene.size_cells.width * $water_cells))
        and ($cell.y | type == "number" and . == floor and . >= 0
          and . < ($root.scene.size_cells.height * $water_cells))
        and ($excavating | index($cell.segment_id) != null)
        and ($cell.floor_meters | type == "number" and isfinite)
        and ($cell.cut_top_meters | type == "number" and isfinite)
        and ($cell.cut_top_meters > $cell.floor_meters)))
  ' "$source_export" >/dev/null \
    || fail "invalid SceneMaker export: $source_export"

  while IFS=$'\t' read -r asset_key expected_type; do
    jq -e --arg key "$asset_key" --arg type "$expected_type" '
      any(.assets[]; .asset_key == $key and .asset_type == $type)
    ' "$asset_catalog" >/dev/null \
      || fail "asset '$asset_key' is not catalogued as '$expected_type' in world01"
    if printf '%s' "$palette_variants" | grep -Fqx "$asset_key"$'\t'"$expected_type"; then
      fail "asset '$asset_key' is a member of a Palette; a map names the Palette itself, so that surface and Placement Rank attach to the choice"
    fi
  done < <(jq -r '
    ([.scene.terrain_cells[].asset_key] | unique | .[] | [., "terrain"] | @tsv),
    ([.scene.route_surfaces[].asset_key] | unique | .[] | [., "terrain"] | @tsv),
    ([.scene.props[].asset_key] | unique | .[] | [., "props"] | @tsv),
    ([.scene.bridges[].plank_asset_key] | unique | .[] | [., "props"] | @tsv),
    ([.scene.bridges[].anchor_asset_key] | unique | .[] | [., "props"] | @tsv)
  ' "$source_export")

  cp "$source_export" "$staging_directory/$(basename "$source_export")"
done

duplicate_scene_ids="$(jq -r '.scene.scene_id' "$staging_directory"/*.scene_export.json | LC_ALL=C sort | uniq -d)"
[[ -z "$duplicate_scene_ids" ]] || fail "duplicate scene IDs: $duplicate_scene_ids"

if [[ -d "$destination_directory" ]]; then
  backup_directory="$(mktemp -d "$project_root/assets/.maps-backup.XXXXXX")"
  rmdir "$backup_directory"
  mv "$destination_directory" "$backup_directory"
fi
mv "$staging_directory" "$destination_directory"
staging_directory=""
if [[ -n "$backup_directory" ]]; then
  rm -rf -- "$backup_directory"
  backup_directory=""
fi
trap - EXIT

printf 'SCENEMAKER -> WORLD01 SYNC SUCCESS (%s exports)\n' "${#source_exports[@]}"

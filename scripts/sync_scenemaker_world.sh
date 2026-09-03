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

source_exports=()
while IFS= read -r source_export; do
  source_exports+=("$source_export")
done < <(find "$source_directory" -maxdepth 1 -type f -name '*.scene_export.json' -print | LC_ALL=C sort)
[[ ${#source_exports[@]} -gt 0 ]] || fail "no SceneMaker exports found in: $source_directory"

staging_directory="$(mktemp -d "$project_root/assets/.maps.XXXXXX")"
for source_export in "${source_exports[@]}"; do
  jq -e '
    . as $root
    | .format == "scene_maker_scene_export"
    and .version == 9
    and .workspace_key == "world01"
    and (.grid.terrain_cell_meters | type == "number" and isfinite and . > 0)
    and (.grid.authoring_pixels_per_meter | type == "number" and isfinite and . > 0)
    and (.grid.game_pixels_per_meter | type == "number" and isfinite and . > 0)
    and (.grid.water_cell_meters | type == "number" and isfinite and . > 0)
    and .scene.schema == "srt.scene_maker_scene"
    and .scene.version == 10
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
    and (if .scene.scene_kind == "instance" then
      .scene.template_definition == null
    else
      (.scene.template_definition | type == "object")
      and (.scene.template_definition.group_number | type == "number" and . == floor and . >= 0)
      and (.scene.template_definition.insertion_anchor_authoring_px.x | type == "number" and . == floor)
      and (.scene.template_definition.insertion_anchor_authoring_px.y | type == "number" and . == floor)
      and (.scene.water_bodies | length == 0)
      and (.water_raster | length == 0)
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
      and (.elevation_meters | type == "number" and isfinite))
    and (([.scene.props[].instance_id] | unique | length) == (.scene.props | length))
    and all(.scene.template_anchors[];
      (.anchor_id | type == "string" and length > 0)
      and (.group_number | type == "number" and . == floor and . >= 0)
      and (.position_authoring_px.x | type == "number" and . == floor)
      and (.position_authoring_px.y | type == "number" and . == floor))
  ' "$source_export" >/dev/null \
    || fail "invalid SceneMaker export: $source_export"

  while IFS=$'\t' read -r asset_key expected_type; do
    jq -e --arg key "$asset_key" --arg type "$expected_type" '
      any(.assets[]; .asset_key == $key and .asset_type == $type)
    ' "$asset_catalog" >/dev/null \
      || fail "asset '$asset_key' is not catalogued as '$expected_type' in world01"
  done < <(jq -r '
    ([.scene.terrain_cells[].asset_key] | unique | .[] | [., "terrain"] | @tsv),
    ([.scene.props[].asset_key] | unique | .[] | [., "props"] | @tsv)
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

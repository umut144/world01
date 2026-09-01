#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_export="${SCENEMAKER_EXPORT:-$project_root/../../GodotProjects/SceneMaker/workspaces/world01/exports/world01.scene_export.json}"
asset_catalog="$project_root/assets/catalog.json"
destination_directory="$project_root/assets/maps"
destination="$destination_directory/world01.scene_export.json"

fail() {
  printf 'SCENEMAKER SYNC FAILED: %s\n' "$1" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || fail 'jq is required.'
[[ -f "$source_export" ]] || fail "export not found: $source_export"
[[ -f "$asset_catalog" ]] || fail "world01 asset catalog not found: $asset_catalog"

jq -e '
  . as $root
  | .format == "scene_maker_scene_export"
  and .version == 5
  and .workspace_key == "world01"
  and (.grid.terrain_cell_meters | type == "number" and isfinite and . > 0)
  and (.grid.authoring_pixels_per_meter | type == "number" and isfinite and . > 0)
  and (.grid.game_pixels_per_meter | type == "number" and isfinite and . > 0)
  and .scene.schema == "srt.scene_maker_scene"
  and .scene.version == 7
  and .scene.scene_id == "world01"
  and .scene.scene_kind == "instance"
  and .scene.coordinate_space == "scene_local_bottom_left_y_up"
  and (.scene.size_cells.width | type == "number" and . == floor and . > 0)
  and (.scene.size_cells.height | type == "number" and . == floor and . > 0)
  and (.scene.terrain_cells | type == "array")
  and (.scene.props | type == "array")
  and (.scene.template_anchors | type == "array")
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
    and (.group_number | type == "number" and . == floor)
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

mkdir -p "$destination_directory"
staging="$(mktemp "$destination_directory/.world01.scene_export.XXXXXX")"
trap 'rm -f -- "$staging"' EXIT
cp "$source_export" "$staging"
mv "$staging" "$destination"
trap - EXIT

printf 'SCENEMAKER -> WORLD01 SYNC SUCCESS\n'

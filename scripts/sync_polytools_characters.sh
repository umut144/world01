#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_world_dir="${POLYTOOLS_WORLD_DIR:-$project_root/../../GodotProjects/PolyTools/worlds/world01}"
source_catalog="$source_world_dir/catalog.json"
assets_dir="$project_root/assets"
required_schema=13

if ! command -v jq >/dev/null 2>&1; then
  printf '%s\n' 'error: jq is required to validate PolyTools exports.' >&2
  exit 1
fi

if [[ ! -f "$source_catalog" ]]; then
  printf 'error: PolyTools catalog not found: %s\n' "$source_catalog" >&2
  exit 1
fi

if ! jq -e '
  .schema_version == 1
  and (.world_key | type == "string")
  and (.world_key | length > 0)
  and (.assets | type == "array")
  and (.assets | length > 0)
  and all(.assets[];
    (.asset_key | type == "string")
    and (.asset_key | length > 0)
    and (.asset_type | type == "string")
    and (.asset_type | length > 0)
    and (.runtime_package | type == "string")
    and (.runtime_package | length > 0)
    and (.asset_type | . == "character" or . == "props" or . == "weapons" or . == "terrain" or . == "icons" or . == "symbols")
  )
  and (([.assets[].asset_key] | unique | length) == ([.assets[].asset_key] | length))
' "$source_catalog" >/dev/null; then
  printf 'error: invalid PolyTools world catalog: %s\n' "$source_catalog" >&2
  exit 1
fi

destination_for_type() {
  case "$1" in
    character) printf '%s\n' 'characters' ;;
    props) printf '%s\n' 'props' ;;
    weapons) printf '%s\n' 'weapons' ;;
    terrain) printf '%s\n' 'terrain' ;;
    icons) printf '%s\n' 'icons' ;;
    symbols) printf '%s\n' 'symbols' ;;
    *)
      printf 'error: unsupported PolyTools asset type: %s\n' "$1" >&2
      exit 1
      ;;
  esac
}

asset_type_for_directory() {
  case "$1" in
    characters) printf '%s\n' 'character' ;;
    props|weapons|terrain|icons|symbols) printf '%s\n' "$1" ;;
    *)
      printf 'error: unsupported destination directory: %s\n' "$1" >&2
      exit 1
      ;;
  esac
}

mkdir -p "$assets_dir"
staging_dir="$(mktemp -d "$assets_dir/.polytools-staging.XXXXXX")"
backup_dir="$(mktemp -d "$assets_dir/.polytools-backup.XXXXXX")"
cleanup() {
  rm -rf "$staging_dir" "$backup_dir"
}
trap cleanup EXIT

for directory in characters props weapons terrain icons symbols; do
  mkdir -p "$staging_dir/$directory"
done

while IFS=$'\t' read -r asset_type asset_key package_path; do
  [[ -n "$asset_key" ]] || continue
  manifest_path="$source_world_dir/$package_path"
  destination_subdir="$(destination_for_type "$asset_type")"

  if [[ ! -f "$manifest_path" ]]; then
    printf 'error: missing PolyTools manifest: %s\n' "$manifest_path" >&2
    exit 1
  fi

  if ! jq -e \
    --arg key "$asset_key" \
    --arg type "$asset_type" \
    --argjson schema "$required_schema" '
      .schema_version == $schema
      and .asset_key == $key
      and .asset_type == $type
      and (.asset_pivot | type == "array" and length == 2 and all(.[]; type == "number" and isfinite))
      and (.components | type == "array" and length > 0)
      and (.components | all(.[];
        (.component_id | type == "string" and length > 0)
        and (.name | type == "string" and length > 0)
        and (.local_transform | type == "object")
      ))
      and (if $type == "character" then
        (.attachment_frames | type == "array")
        and ([.components[] | select(
          (.name == "body" or .name == "head")
          and (.mesh | type == "object")
          and (.mesh.vertices | type == "array" and length >= 3)
          and (.mesh.indices | type == "array" and length >= 3 and length % 3 == 0)
        )] | length == 2)
      elif $type == "weapons" and $key == "hammer" then
        (.attachment_frames | type == "array")
        and ([.attachment_frames[] | select(.role == "grip_primary")] | length == 1)
        and ([.attachment_frames[] | select(.role == "grip_secondary")] | length == 1)
        and ([.attachment_frames[] | select(.role == "attack_point_primary")] | length == 1)
        and ([.attachment_frames[] | select(.role == "reach_limit_primary")] | length == 1)
        and ([.components[] | select(
          (.name == "head_mid" or .name == "head_left" or .name == "head_right")
          and (.mesh | type == "object")
          and (.mesh.vertices | type == "array" and length >= 3)
          and (.mesh.indices | type == "array" and length >= 3 and length % 3 == 0)
        )] | length == 3)
      else true end)
    ' "$manifest_path" >/dev/null; then
    printf 'error: invalid PolyTools %s manifest: %s\n' "$asset_type" "$manifest_path" >&2
    exit 1
  fi

  mkdir -p "$staging_dir/$destination_subdir/$asset_key"
  cp "$manifest_path" "$staging_dir/$destination_subdir/$asset_key/manifest.json"
done < <(jq -r '.assets[] | [.asset_type, .asset_key, .runtime_package] | @tsv' "$source_catalog" | sort)

cp "$source_catalog" "$staging_dir/catalog.json"
for directory in characters props weapons terrain icons symbols; do
  asset_type="$(asset_type_for_directory "$directory")"
  jq --arg type "$asset_type" '
    .assets |= map(select(.asset_type == $type))
  ' "$source_catalog" >"$staging_dir/$directory/catalog.json"
done

for directory in characters props weapons terrain icons symbols; do
  if [[ -e "$assets_dir/$directory" ]]; then
    mv "$assets_dir/$directory" "$backup_dir/$directory"
  fi
  mv "$staging_dir/$directory" "$assets_dir/$directory"
done

if [[ -e "$assets_dir/catalog.json" ]]; then
  mv "$assets_dir/catalog.json" "$backup_dir/catalog.json"
fi
mv "$staging_dir/catalog.json" "$assets_dir/catalog.json"

printf 'Synced PolyTools world assets (characters, props, weapons, terrain, icons, symbols) to %s\n' "$assets_dir"

#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_world_dir="${POLYTOOLS_WORLD_DIR:-$project_root/../../GodotProjects/PolyTools/worlds/world01}"
source_catalog="$source_world_dir/catalog.json"
assets_dir="$project_root/assets"
required_schema=13

success_color=''
success_reset=''
error_color=''
warning_color=''
error_reset=''
if [[ -z "${NO_COLOR:-}" && -t 1 ]]; then
  success_color=$'\033[1;32m'
  success_reset=$'\033[0m'
fi
if [[ -z "${NO_COLOR:-}" && -t 2 ]]; then
  error_color=$'\033[1;31m'
  warning_color=$'\033[1;33m'
  error_reset=$'\033[0m'
fi

error_message() {
  printf '%bERROR: %s%b\n' "$error_color" "$1" "$error_reset" >&2
}

warning_message() {
  printf '%bWARNING: %s%b\n' "$warning_color" "$1" "$error_reset" >&2
}

finish() {
  local status=$?
  trap - EXIT
  if [[ -n "${staging_dir:-}" ]]; then
    rm -rf -- "$staging_dir"
  fi
  if [[ -n "${backup_dir:-}" ]]; then
    rm -rf -- "$backup_dir"
  fi
  if ((status == 0)); then
    printf '%b%s%b\n' "$success_color" 'POLYTOOLS SYNC SUCCESS' "$success_reset"
  else
    printf '%b%s%b\n' "$error_color" 'POLYTOOLS SYNC FAILED' "$error_reset" >&2
  fi
  exit "$status"
}
trap finish EXIT

if ! command -v jq >/dev/null 2>&1; then
  error_message 'jq is required to validate PolyTools exports.'
  exit 1
fi

if [[ ! -f "$source_catalog" ]]; then
  error_message "PolyTools catalog not found: $source_catalog"
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
  error_message "invalid PolyTools world catalog: $source_catalog"
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
      error_message "unsupported PolyTools asset type: $1"
      exit 1
      ;;
  esac
}

asset_type_for_directory() {
  case "$1" in
    characters) printf '%s\n' 'character' ;;
    props|weapons|terrain|icons|symbols) printf '%s\n' "$1" ;;
    *)
      error_message "unsupported destination directory: $1"
      exit 1
      ;;
  esac
}

mkdir -p "$assets_dir"
staging_dir="$(mktemp -d "$assets_dir/.polytools-staging.XXXXXX")"
backup_dir="$(mktemp -d "$assets_dir/.polytools-backup.XXXXXX")"

for directory in characters props weapons terrain icons symbols; do
  mkdir -p "$staging_dir/$directory"
done

while IFS=$'\t' read -r asset_type asset_key package_path; do
  [[ -n "$asset_key" ]] || continue
  manifest_path="$source_world_dir/$package_path"
  destination_subdir="$(destination_for_type "$asset_type")"

  if [[ ! -f "$manifest_path" ]]; then
    error_message "missing PolyTools manifest: $manifest_path"
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
    error_message "invalid PolyTools $asset_type manifest: $manifest_path"
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

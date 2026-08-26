#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
source_world_dir="${POLYTOOLS_WORLD_DIR:-$project_root/../../GodotProjects/PolyTools/worlds/world01}"
source_catalog="$source_world_dir/catalog.json"
destination_dir="$project_root/assets/characters"
character_keys=()
symbol_keys=()
while IFS= read -r key; do
  [[ -n "$key" ]] && character_keys+=("$key")
done < <(jq -r '.assets[] | select(.asset_type == "character") | .asset_key' "$source_catalog" | sort)
if [[ "${#character_keys[@]}" -eq 0 ]]; then
  printf '%s\n' 'error: catalog contains no character assets.' >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  printf '%s\n' 'error: jq is required to validate PolyTools character exports.' >&2
  exit 1
fi

if [[ ! -f "$source_catalog" ]]; then
  printf 'error: PolyTools catalog not found: %s\n' "$source_catalog" >&2
  exit 1
fi

if ! jq -e '
  .schema_version == 1
  and (.assets | type == "array")
  and (.world_key | type == "string" and length > 0)
' "$source_catalog" >/dev/null; then
  printf 'error: invalid PolyTools catalog: %s\n' "$source_catalog" >&2
  exit 1
fi

mkdir -p "$project_root/assets"
staging_dir="$(mktemp -d "$project_root/assets/.characters-staging.XXXXXX")"
cleanup() {
  rm -rf "$staging_dir"
}
trap cleanup EXIT

for key in "${character_keys[@]}"; do
  package_path="PolyToolsRuntimeExports/$key/manifest.json"
  manifest_path="$source_world_dir/$package_path"

  if ! jq -e --arg key "$key" --arg package_path "$package_path" '
    [.assets[] | select(
      .asset_key == $key
      and .asset_type == "character"
      and .runtime_package == $package_path
    )] | length == 1
  ' "$source_catalog" >/dev/null; then
    printf 'error: catalog has no valid character package for %s.\n' "$key" >&2
    exit 1
  fi

  if [[ ! -f "$manifest_path" ]]; then
    printf 'error: missing PolyTools manifest: %s\n' "$manifest_path" >&2
    exit 1
  fi

  if ! jq -e --arg key "$key" '
    .schema_version == 6
    and .asset_key == $key
    and .asset_type == "character"
    and (.components | type == "array" and length > 0)
  ' "$manifest_path" >/dev/null; then
    printf 'error: invalid PolyTools character manifest: %s\n' "$manifest_path" >&2
    exit 1
  fi

  while IFS= read -r symbol_key; do
    [[ -n "$symbol_key" ]] && symbol_keys+=("$symbol_key")
  done < <(jq -r '.components[] | .source_asset_key // empty' "$manifest_path" | sort -u)

  mkdir "$staging_dir/$key"
  cp "$manifest_path" "$staging_dir/$key/manifest.json"
done

for key in "${symbol_keys[@]}"; do
  package_path="PolyToolsRuntimeExports/$key/manifest.json"
  manifest_path="$source_world_dir/$package_path"
  [[ -f "$manifest_path" ]] || { printf 'error: missing referenced PolyTools manifest: %s\n' "$manifest_path" >&2; exit 1; }
  mkdir -p "$staging_dir/$key"
  cp "$manifest_path" "$staging_dir/$key/manifest.json"
done

jq --argjson keys "$(printf '%s\n' "${character_keys[@]}" "${symbol_keys[@]}" | jq -R . | jq -s .)" '
  .assets |= [
    .[]
    | select(.asset_key as $key | $keys | index($key))
  ]
' "$source_catalog" >"$staging_dir/catalog.json"

if [[ -e "$destination_dir" ]]; then
  backup_dir="${destination_dir}.previous"
  rm -rf "$backup_dir"
  mv "$destination_dir" "$backup_dir"
  if mv "$staging_dir" "$destination_dir"; then
    rm -rf "$backup_dir"
  else
    mv "$backup_dir" "$destination_dir"
    exit 1
  fi
else
  mv "$staging_dir" "$destination_dir"
fi

trap - EXIT
printf 'Synced PolyTools character exports to %s\n' "$destination_dir"

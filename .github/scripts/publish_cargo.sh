#!/usr/bin/env bash
set -euo pipefail

workspace_manifest="${1:?usage: publish_cargo.sh <workspace Cargo.toml> [dry-run]}"
dry_run="${2:-}"

workspace_root="$(cd "$(dirname "$workspace_manifest")" && pwd)"
manifest_path="$workspace_root/$(basename "$workspace_manifest")"
output_target="$(mktemp)"
trap 'rm -f "$output_target"' EXIT

read_package_version() {
  local crate_manifest="$1"
  awk '
    /^\[package\]/ { in_package = 1; next }
    /^\[/ { if (in_package) exit }
    in_package && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"/ {
      value = $0
      sub(/^[^"]*"/, "", value)
      sub(/".*/, "", value)
      print value
      exit
    }
  ' "$crate_manifest"
}

read_package_name() {
  local crate_manifest="$1"
  awk '
    /^\[package\]/ { in_package = 1; next }
    /^\[/ { if (in_package) exit }
    in_package && /^[[:space:]]*name[[:space:]]*=[[:space:]]*"/ {
      value = $0
      sub(/^[^"]*"/, "", value)
      sub(/".*/, "", value)
      print value
      exit
    }
  ' "$crate_manifest"
}

registry_already_has() {
  local crate_name="$1"
  local crate_version="$2"
  local status_code
  status_code="$(curl -s -o /dev/null -w '%{http_code}' -A 'ygopru-publish' "https://crates.io/api/v1/crates/$crate_name/$crate_version")"
  [[ "$status_code" == "200" ]]
}

read_upstream_ref() {
  local crate_directory="$1"
  local upstream_directory="$crate_directory/ocgcore"
  if [[ -d "$upstream_directory/.git" || -f "$upstream_directory/.git" ]]; then
    git -C "$upstream_directory" rev-parse HEAD 2>/dev/null
  fi
}

set_package_version() {
  local crate_manifest="$1"
  local new_version="$2"
  local temp_file
  temp_file="$(mktemp)"
  awk -v new_version="$new_version" '
    /^\[package\]/ { in_package = 1 }
    /^\[/ && $0 != "[package]" && in_package { in_package = 0 }
    in_package && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"/ {
      print "version = \"" new_version "\""
      next
    }
    { print }
  ' "$crate_manifest" > "$temp_file"
  cp "$temp_file" "$crate_manifest"
  rm -f "$temp_file"
}

prepare_upstream_suffixes() {
  local prepared_entries=()
  local entry
  for entry in "${publish_entries[@]}"; do
    local crate_name="${entry%%|*}"
    local entry_rest="${entry#*|}"
    local crate_directory="${entry_rest%%|*}"
    local crate_version="${entry_rest#*|}"
    local upstream_ref
    upstream_ref="$(read_upstream_ref "$crate_directory")"
    if [[ -n "$upstream_ref" ]]; then
      crate_version="${crate_version%%+*}+official.${upstream_ref}"
      set_package_version "$crate_directory/Cargo.toml" "$crate_version"
      echo "upstream $crate_name labeled $crate_version"
    elif [[ -d "$crate_directory/ocgcore/.git" || -f "$crate_directory/ocgcore/.git" ]]; then
      echo "cannot read upstream commit for $crate_name at $crate_directory/ocgcore" >&2
      exit 1
    fi
    prepared_entries+=("$crate_name|$crate_directory|$crate_version")
  done
  publish_entries=("${prepared_entries[@]}")
}

publish_one() {
  local crate_name="$1"
  local crate_version="$2"
  local publish_log
  publish_log="$(cargo publish --manifest-path "$manifest_path" --package "$crate_name" --allow-dirty 2>&1)"
  local publish_status=$?
  if [[ $publish_status -eq 0 ]]; then
    return 0
  fi
  if printf '%s' "$publish_log" | grep -qE 'already exists|already uploaded'; then
    if registry_already_has "$crate_name" "$crate_version"; then
      return 0
    fi
    failure_logs["$crate_name"]="version conflict: a different $crate_name with the same base version is on crates.io; bump the version before the +official suffix"
    return 1
  fi
  failure_logs["$crate_name"]="$publish_log"
  return 1
}

publish_all() {
  declare -A failure_logs
  local remaining_packages=("${publish_entries[@]}")
  local wave
  for (( wave = 0; wave < 12; wave++ )); do
    if (( ${#remaining_packages[@]} == 0 )); then
      return 0
    fi
    local still_pending=()
    local made_progress=0
    local entry
    for entry in "${remaining_packages[@]}"; do
      local crate_name="${entry%%|*}"
      local entry_rest="${entry#*|}"
      local crate_version="${entry_rest#*|}"
      if registry_already_has "$crate_name" "$crate_version"; then
        echo "skip $crate_name@$crate_version"
        made_progress=1
        continue
      fi
      if publish_one "$crate_name" "$crate_version"; then
        echo "published $crate_name@$crate_version"
        made_progress=1
        sleep 2
        continue
      fi
      still_pending+=("$entry")
    done
    remaining_packages=("${still_pending[@]}")
    if (( made_progress == 0 )); then
      echo "publish stalled for: ${remaining_packages[*]}" >&2
      for entry in "${remaining_packages[@]}"; do
        local crate_name="${entry%%|*}"
        echo "--- $crate_name ---" >&2
        printf '%s\n' "${failure_logs[$crate_name]:-no log}" >&2
      done
      exit 1
    fi
  done
  if (( ${#remaining_packages[@]} != 0 )); then
    echo "publish did not finish: ${remaining_packages[*]}" >&2
    for entry in "${remaining_packages[@]}"; do
      local crate_name="${entry%%|*}"
      echo "--- $crate_name ---" >&2
      printf '%s\n' "${failure_logs[$crate_name]:-no log}" >&2
    done
    exit 1
  fi
}

in_dependencies=0
publish_entries=()
while IFS= read -r line; do
  if [[ "$line" == "[workspace.dependencies]" ]]; then
    in_dependencies=1
  elif [[ "$line" =~ ^\[ ]]; then
    in_dependencies=0
  fi

  entry_regex='^[[:space:]]*([A-Za-z0-9_-]+)[[:space:]]*=[[:space:]]*\{[[:space:]]*path[[:space:]]*=[[:space:]]*"\./([^"]+)"'
  if (( in_dependencies )); then
    if [[ "$line" =~ $entry_regex ]]; then
      dependency_name="${BASH_REMATCH[1]}"
      crate_directory="$workspace_root/${BASH_REMATCH[2]}"
      crate_manifest="$crate_directory/Cargo.toml"
      if [[ -f "$crate_manifest" ]]; then
        actual_name="$(read_package_name "$crate_manifest")"
        if [[ -n "$actual_name" && "$actual_name" != "$dependency_name" ]]; then
          echo "dependency $dependency_name points to $crate_directory whose package is named $actual_name" >&2
          exit 1
        fi
        version="$(read_package_version "$crate_manifest")"
        if [[ -z "$version" ]]; then
          echo "$crate_manifest has no literal [package] version; declare version = \"x.y.z\" there" >&2
          exit 1
        fi
        cleaned="$(printf '%s' "$line" | sed -E 's/,?[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]*"//')"
        cleaned="$(printf '%s' "$cleaned" | sed -E 's/[[:space:]]+$//')"
        cleaned="${cleaned%\}}"
        line="${cleaned}, version = \"$version\" }"
        line="${line// ,/,}"
        publish_entries+=("$dependency_name|$crate_directory|$version")
      fi
    fi
  fi
  printf '%s\n' "$line"
done < "$manifest_path" > "$output_target"

if [[ "$dry_run" == "dry-run" ]]; then
  cat "$output_target"
else
  cp "$output_target" "$manifest_path"
  if [[ "$dry_run" == "publish" ]]; then
    prepare_upstream_suffixes
    publish_all
  fi
fi

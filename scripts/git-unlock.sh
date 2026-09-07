#!/bin/sh
# Clear Git lock and temporary object files that a crashed or sandboxed process
# left behind.
#
#   ./scripts/git-unlock.sh
#
# A lock file is Git's way of saying "someone is writing here right now", so
# removing a live one destroys another process's work. This script therefore
# removes a file only when both hold:
#
#   * it is older than GIT_UNLOCK_AGE seconds (default 15), and
#   * no process holds it open (checked with lsof).
#
# Anything else is left alone and reported. Without lsof, or when the file's
# age cannot be determined, nothing is removed: refusing is the safe direction.
#
# Only locks and temporary objects are ever touched:
#
#   <git-dir>/*.lock                     (HEAD.lock, index.lock, config.lock …)
#   <git-dir>/{objects,refs,logs,modules}/**/*.lock
#   <git-dir>/{objects,modules}/**/tmp_obj_*
#
# Nothing else below the Git directory is a candidate, ever.
#
# Where deletion is not permitted — a sandboxed working copy, for instance —
# the file is moved into _to_delete/git-leftovers/ under a name that does not
# collide, which clears the lock just as well.
#
# Exit status: 0 when no lock is left in the way, non-zero when one remains and
# the developer has to look at it.
#
# Environment:
#   GIT_UNLOCK_AGE   seconds a file must be untouched  (default 15)
#   GIT_UNLOCK_PARK  where undeletable files are moved (default
#                    <toplevel>/_to_delete/git-leftovers)

set -eu

age_limit=${GIT_UNLOCK_AGE:-15}
case "$age_limit" in ''|*[!0-9]*)
  echo "git-unlock: GIT_UNLOCK_AGE must be a whole number of seconds" >&2
  exit 2 ;;
esac

git_dir=$(git rev-parse --absolute-git-dir 2>/dev/null) || git_dir=""
if [ -z "$git_dir" ]; then
  git_dir=$(git rev-parse --git-dir 2>/dev/null) || {
    echo "git-unlock: not inside a git repository" >&2
    exit 2
  }
  case "$git_dir" in /*) ;; *) git_dir=$(cd "$git_dir" && pwd) ;; esac
fi

top=$(git rev-parse --show-toplevel 2>/dev/null) || top=""
[ -n "$top" ] || top=$(dirname "$git_dir")
park=${GIT_UNLOCK_PARK:-$top/_to_delete/git-leftovers}

mtime() {
  # GNU stat first: on Linux, BSD's -f means something else and succeeds with
  # unusable output.
  m=$(stat -c %Y "$1" 2>/dev/null) || m=""
  case "$m" in ''|*[!0-9]*) m=$(stat -f %m "$1" 2>/dev/null) || m="" ;; esac
  case "$m" in ''|*[!0-9]*) return 1 ;; esac
  printf '%s\n' "$m"
}

short() {
  # path relative to the git directory, for readable output
  case "$1" in
    "$git_dir"/*) printf '%s\n' "${1#"$git_dir"/}" ;;
    *) printf '%s\n' "$1" ;;
  esac
}

list=$(mktemp) || { echo "git-unlock: cannot create a temporary file" >&2; exit 2; }
trap 'rm -f "$list" 2>/dev/null || true' EXIT INT TERM

find "$git_dir" -maxdepth 1 -type f -name '*.lock' -print > "$list" 2>/dev/null || true
for sub in objects refs logs modules; do
  [ -d "$git_dir/$sub" ] || continue
  case "$sub" in
    objects|modules)
      find "$git_dir/$sub" -type f \( -name '*.lock' -o -name 'tmp_obj_*' \) -print
        ;;
    *)
      find "$git_dir/$sub" -type f -name '*.lock' -print
        ;;
  esac >> "$list" 2>/dev/null || true
done

total=$(wc -l < "$list" | tr -d ' ')
if [ "$total" -eq 0 ]; then
  printf 'git-unlock: nothing to clear in %s\n' "$git_dir"
  exit 0
fi

have_lsof=1
command -v lsof >/dev/null 2>&1 || have_lsof=0
if [ "$have_lsof" -eq 0 ]; then
  printf 'git-unlock: lsof is missing, so no file can be shown to be unused.\n'
  printf 'git-unlock: leaving all %s candidates untouched.\n' "$total"
fi

printf 'git-unlock: %s candidate(s) under %s\n' "$total" "$git_dir"

now=$(date +%s)
removed=0
parked=0
kept=0

while IFS= read -r file; do
  [ -n "$file" ] || continue
  [ -e "$file" ] || continue
  name=$(short "$file")

  if [ "$have_lsof" -eq 0 ]; then
    printf '  kept     %s (lsof missing, cannot prove it is unused)\n' "$name"
    kept=$(( kept + 1 ))
    continue
  fi

  stamp=$(mtime "$file") || {
    printf '  kept     %s (cannot read its age)\n' "$name"
    kept=$(( kept + 1 ))
    continue
  }
  age=$(( now - stamp ))
  if [ "$age" -lt "$age_limit" ]; then
    printf '  kept     %s (only %ss old, limit %ss)\n' "$name" "$age" "$age_limit"
    kept=$(( kept + 1 ))
    continue
  fi

  holders=$(lsof -t -- "$file" 2>/dev/null | tr '\n' ' ' | sed 's/ *$//') || holders=""
  if [ -n "$holders" ]; then
    printf '  kept     %s (held open by pid %s)\n' "$name" "$holders"
    kept=$(( kept + 1 ))
    continue
  fi

  if rm -f "$file" 2>/dev/null && [ ! -e "$file" ]; then
    printf '  removed  %s\n' "$name"
    removed=$(( removed + 1 ))
    continue
  fi

  mkdir -p "$park" 2>/dev/null || true
  base=$(printf '%s' "$name" | tr '/' '_')
  dest="$park/$base"
  n=0
  while [ -e "$dest" ]; do
    n=$(( n + 1 ))
    dest="$park/$base.$n"
  done
  if mv "$file" "$dest" 2>/dev/null; then
    printf '  parked   %s -> %s\n' "$name" "$dest"
    parked=$(( parked + 1 ))
  else
    printf '  kept     %s (can neither be removed nor moved)\n' "$name"
    kept=$(( kept + 1 ))
  fi
done < "$list"

printf 'git-unlock: %s removed, %s parked, %s kept\n' "$removed" "$parked" "$kept"

# Only locks block Git; leftover temporary objects are untidy, not in the way.
left=$(
  {
    find "$git_dir" -maxdepth 1 -type f -name '*.lock' -print
    for sub in objects refs logs modules; do
      [ -d "$git_dir/$sub" ] || continue
      find "$git_dir/$sub" -type f -name '*.lock' -print
    done
  } 2>/dev/null | wc -l | tr -d ' '
)

if [ "$left" -eq 0 ]; then
  printf 'git-unlock: no lock left in the way\n'
  exit 0
fi

printf 'git-unlock: %s lock(s) still in place; ask the developer before forcing anything\n' "$left" >&2
exit 1

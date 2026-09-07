#!/bin/sh
# Watcher for agent-triggered validation runs.
#
# check-agent protocol 2
#
# Start this once in a terminal tab and leave it running:
#
#   checkw start          (or ./scripts/check-agent.sh)
#
# It serves one queue of requests, so several agent sessions can share it. The
# state lives in .agent-check/ :
#
#   requests/<id>   "args=<arguments>"   written by check-agent-run.sh
#   pending/<sess>  the id a session waits for
#   results/<id>    header + output of that run
#   logs/<id>.log   raw output of that run
#   result, last.log   copy of the most recent run, whoever asked for it;
#                      a developer view, not read by any agent
#
# The oldest request is served first. A request is removed only after its log
# and result are in place, so an agent that finds its request gone and no
# result knows the run was dropped rather than waiting on.
#
# Environment:
#   CHECK_AGENT_DIR       state directory          (default .agent-check)
#   CHECK_AGENT_INTERVAL  poll interval seconds    (default 2)
#   CHECK_AGENT_KEEP      runs kept in results/logs (default 50)
#   CHECK_AGENT_STALE     drop unread requests older than N s (default 3600)
#   CHECK_SCRIPT          script to run            (default ./scripts/check.sh)
#   CHECK_SOUND=0         disable the sound
#   CHECK_SOUND_OK / CHECK_SOUND_FAIL   sound files
#   CHECK_CLIPBOARD=0     do not copy output to the clipboard

set -eu

# Layout of requests/ and of the result header. Bump this whenever either
# changes, and keep it equal to the constant in check-agent-run.sh: it is
# written into every result so a session can tell a watcher process that still
# runs an older script from a current one.
protocol=2

root=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "check-agent: not inside a git worktree" >&2
  exit 127
}
cd "$root"

dir=${CHECK_AGENT_DIR:-.agent-check}
interval=${CHECK_AGENT_INTERVAL:-2}
keep=${CHECK_AGENT_KEEP:-50}
stale=${CHECK_AGENT_STALE:-3600}
script=${CHECK_SCRIPT:-./scripts/check.sh}

[ -x "$script" ] || { echo "check-agent: $script is not executable" >&2; exit 127; }
mkdir -p "$dir/requests" "$dir/pending" "$dir/results" "$dir/logs"

# Leftovers of the single-slot protocol; the shell side switches over as soon
# as requests/ exists.
rm -f "$dir/request" "$dir/pending-id" "$dir/processed"

mtime() {
  # GNU stat first: BSD's -f means something else there and succeeds with
  # unusable output, which used to kill the watcher in the arithmetic below.
  m=$(stat -c %Y "$1" 2>/dev/null) || m=""
  case "$m" in ''|*[!0-9]*) m=$(stat -f %m "$1" 2>/dev/null) || m="" ;; esac
  case "$m" in ''|*[!0-9]*) m=0 ;; esac
  printf '%s\n' "$m"
}

# A run blocks the loop, so the heartbeat gets its own process: a session must
# be able to tell "watcher busy" from "watcher gone" during a long run.
( while :; do date +%s > "$dir/heartbeat" 2>/dev/null || exit 0; sleep "$interval"; done ) &
heartbeat_pid=$!
cleanup() { kill "$heartbeat_pid" 2>/dev/null || true; }
trap 'cleanup; printf "\ncheck-agent: stopped\n"; exit 0' INT TERM
trap cleanup EXIT

printf 'check-agent: serving %s\n' "$dir/requests"
printf 'check-agent: runs %s per request, Ctrl-C to stop.\n\n' "$script"

while :; do
  # Oldest request first; .part files are still being written.
  id=$(ls -tr "$dir/requests" 2>/dev/null | grep -v '\.part$' | head -n 1 || true)

  if [ -n "$id" ]; then
    request="$dir/requests/$id"
    args=$(sed -n 's/^args=//p' "$request" 2>/dev/null | tail -n 1 || true)

    if ! grep -q '^args=' "$request" 2>/dev/null; then
      # Still mid-write, or malformed: give it one interval, then drop it.
      if [ $(( $(date +%s) - $(mtime "$request") )) -gt 10 ]; then
        printf 'check-agent: dropping unreadable request %s\n' "$id"
        rm -f "$request"
      fi
      sleep "$interval"
      continue
    fi

    # A request from a session that is long gone must not be executed, however
    # far down the queue it sits: the queue is served oldest first, so this is
    # where staleness is decided.
    if [ $(( $(date +%s) - $(mtime "$request") )) -gt "$stale" ]; then
      printf 'check-agent: dropping stale request %s\n' "$id"
      rm -f "$request"
      continue
    fi

    printf '\n▶ check-agent: request %s  (args: %s)\n' "$id" "${args:-<none>}"
    started=$(date +%s)
    log="$dir/logs/$id.log"

    # What this run is about to test. Taken before the run, because that is the
    # tree it sees. `--no-optional-locks` matters: without it this very query
    # writes .git/index.lock and collides with whoever else is working here.
    head=$(git --no-optional-locks rev-parse --short HEAD 2>/dev/null || echo unknown)
    worktree=$(git --no-optional-locks status --porcelain 2>/dev/null || true)
    dirty=no
    [ -z "$worktree" ] || dirty=yes

    set +e
    # Word splitting is intended: args is a plain argument list.
    # shellcheck disable=SC2086
    "$script" $args > "$log.tmp" 2>&1
    rc=$?
    set -e
    mv -f "$log.tmp" "$log"

    duration=$(( $(date +%s) - started ))
    lines=$(wc -l < "$log" | tr -d ' ')

    {
      printf 'id=%s\n' "$id"
      printf 'exit=%s\n' "$rc"
      printf 'args=%s\n' "$args"
      printf 'duration=%s\n' "$duration"
      printf 'lines=%s\n' "$lines"
      printf 'protocol=%s\n' "$protocol"
      printf 'head=%s\n' "$head"
      printf 'dirty=%s\n' "$dirty"
      printf -- '--- output ---\n'
      cat "$log"
    } > "$dir/results/$id.tmp"
    mv -f "$dir/results/$id.tmp" "$dir/results/$id"

    # Only now: whoever waits on this id can already find the result.
    rm -f "$request"

    for p in "$dir"/pending/*; do
      [ -f "$p" ] || continue
      [ "$(cat "$p" 2>/dev/null || true)" = "$id" ] && rm -f "$p"
    done

    cp "$log" "$dir/last.log.tmp" && mv -f "$dir/last.log.tmp" "$dir/last.log"
    cp "$dir/results/$id" "$dir/result.tmp" && mv -f "$dir/result.tmp" "$dir/result"

    if [ "${CHECK_CLIPBOARD:-1}" != 0 ] && command -v pbcopy >/dev/null 2>&1; then
      pbcopy < "$log" 2>/dev/null || true
    fi

    if [ "$rc" -eq 0 ]; then
      printf '✓ check.sh %s – exit %s, %ss, %s Zeilen\n' "$args" "$rc" "$duration" "$lines"
      sound=${CHECK_SOUND_OK:-/System/Library/Sounds/Glass.aiff}
    else
      printf '✗ check.sh %s – exit %s, %ss, %s Zeilen\n' "$args" "$rc" "$duration" "$lines"
      sound=${CHECK_SOUND_FAIL:-/System/Library/Sounds/Basso.aiff}
    fi
    waiting=$(ls "$dir/requests" 2>/dev/null | grep -cv '\.part$' || true)
    printf '  → %s (offen: %s)\n' "$log" "${waiting:-0}"

    if [ "${CHECK_SOUND:-1}" != 0 ] && command -v afplay >/dev/null 2>&1 && [ -f "$sound" ]; then
      afplay "$sound" >/dev/null 2>&1 &
    fi

    # Housekeeping between runs, never while one is waiting to start. It runs in
    # a subshell and its failure is ignored on purpose: a watcher that dies here
    # would leave every session waiting on a queue nobody serves.
    (
      now=$(date +%s)
      for f in "$dir"/requests/*; do
        [ -f "$f" ] || continue
        [ $(( now - $(mtime "$f") )) -gt "$stale" ] || continue
        printf 'check-agent: dropping stale request %s\n' "$(basename "$f")"
        rm -f "$f"
      done
      for d in "$dir/results" "$dir/logs"; do
        ls -t "$d" 2>/dev/null | tail -n +$(( keep + 1 )) | while IFS= read -r f; do
          rm -f "$d/$f"
        done
      done
    ) || true

    continue
  fi

  sleep "$interval"
done

#!/bin/sh
# Agent side of the validation loop: put a request into the queue that the
# running check-agent watcher serves, and report the result.
#
#   ./scripts/check-agent-run.sh --tests     request a run and wait for it
#   ./scripts/check-agent-run.sh --poll      keep waiting for this session's run
#   ./scripts/check-agent-run.sh --full      print the whole output, not a tail
#
# Exit codes:
#   0   run finished, check.sh succeeded
#   1   run finished, check.sh failed (its exit code is in the header)
#   2   the call itself is wrong: --poll without a request of this session,
#       or an argument outside CHECK_AGENT_ARGS. Fix the call, do not repeat
#       it unchanged.
#   3   no watcher running (start it with `checkw start`)
#   4   still running, call again with --poll
#   5   the request is gone without a result; request again, do not wait
#
# Several sessions share one watcher, so a session is identified by
# CHECK_AGENT_SESSION. Without it the identity is derived from the physical
# path of the worktree, which is stable per sandbox mount and per checkout;
# $PPID is not usable here because every call may run in a fresh shell.
#
# Environment:
#   CHECK_AGENT_DIR      state directory        (default .agent-check)
#   CHECK_AGENT_SESSION  session identity       (default: derived, see above)
#   CHECK_AGENT_WAIT     seconds to wait        (default 90)
#   CHECK_AGENT_TAIL     tail lines to print    (default 80)
#   CHECK_AGENT_FAIL     regex of a first failure  (default error|FAILED|Diff in)
#   CHECK_AGENT_FAIL_LINES  lines shown from it    (default 40)
#   CHECK_AGENT_ARGS     arguments a request may carry (default --tests)

set -eu

root=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "check-agent-run: not inside a git worktree" >&2
  exit 127
}
cd "$root"

dir=${CHECK_AGENT_DIR:-.agent-check}
wait_secs=${CHECK_AGENT_WAIT:-90}
tail_lines=${CHECK_AGENT_TAIL:-80}
# Where a failing run went wrong is usually where it FIRST went wrong, and the
# words for that belong to the project, not to this script.
fail_pattern=${CHECK_AGENT_FAIL:-error|FAILED|Diff in}
fail_lines=${CHECK_AGENT_FAIL_LINES:-40}
# Which arguments a request may carry. This keeps a typo from becoming a
# command; it is not a defence against anyone, because whoever can write a
# request here can already run anything on this machine.
allowed_args=${CHECK_AGENT_ARGS:---tests}

session=${CHECK_AGENT_SESSION:-}
if [ -n "$session" ]; then
  session=$(printf '%s' "$session" | tr -c 'A-Za-z0-9._-' '-')
else
  session=$(pwd -P | cksum | cut -d' ' -f1)
fi

mode=new
full=0
args=""
for a in "$@"; do
  case "$a" in
    --poll) mode=poll ;;
    --full) full=1 ;;
    *) args="${args:+$args }$a" ;;
  esac
done

# A wrong argument is wrong whether or not a watcher is listening, so it is
# said here rather than after the queue has been found.
for a in $args; do
  known=0
  for allowed in $allowed_args; do
    [ "$a" = "$allowed" ] && known=1
  done
  if [ "$known" -eq 0 ]; then
    echo "check-agent-run: unbekanntes Argument '$a'" >&2
    echo "erlaubt sind: ${allowed_args:-<keine>} (CHECK_AGENT_ARGS)" >&2
    exit 2
  fi
done

read_field() {
  [ -f "$1" ] || return 0
  sed -n "s/^$2=//p" "$1" | tail -n 1
}

report() {
  # report <id>
  sed -n '1,/^--- output ---$/p' "$dir/results/$1"
  log="$dir/logs/$1.log"
  rc=$(read_field "$dir/results/$1" exit)
  case "$rc" in ''|*[!0-9]*) rc=1 ;; esac
  total=$(read_field "$dir/results/$1" lines)
  case "$total" in ''|*[!0-9]*) total=0 ;; esac

  if [ "$full" -eq 1 ]; then
    cat "$log" 2>/dev/null || true
  elif [ "$rc" -eq 0 ]; then
    if [ "$total" -gt "$tail_lines" ]; then
      printf '(letzte %s von %s Zeilen; komplett: %s)\n' "$tail_lines" "$total" "$log"
    fi
    tail -n "$tail_lines" "$log" 2>/dev/null || true
  else
    # A run stops at the first thing it could not do, so that is what is worth
    # reading. Without a hit the beginning is still a better guess than the end.
    first=$(grep -n -E "$fail_pattern" "$log" 2>/dev/null | head -n 1 | cut -d: -f1 || true)
    case "$first" in ''|*[!0-9]*) first=1 ;; esac
    last=$(( first + fail_lines - 1 ))
    printf '(Zeilen %s-%s von %s ab dem ersten Treffer; komplett: %s)\n' \
      "$first" "$last" "$total" "$log"
    sed -n "${first},${last}p" "$log" 2>/dev/null || true
  fi

  [ "$rc" -eq 0 ] && exit 0
  exit 1
}

if [ ! -d "$dir/requests" ]; then
  echo "WATCHER-OFFLINE: $dir/requests fehlt, es lief hier noch nie ein Watcher." >&2
  echo "Bitte im Projektroot in einem Terminal-Tab starten:  checkw start" >&2
  exit 3
fi

now=$(date +%s)
beat=$(cat "$dir/heartbeat" 2>/dev/null || echo 0)
case "$beat" in ''|*[!0-9]*) beat=0 ;; esac
age=$(( now - beat ))
if [ "$age" -gt 15 ] || [ "$age" -lt -15 ]; then
  echo "WATCHER-OFFLINE: kein Heartbeat (letzter vor ${age}s)." >&2
  echo "Bitte im Projektroot in einem Terminal-Tab starten:  checkw start" >&2
  exit 3
fi

mkdir -p "$dir/pending"

if [ "$mode" = new ]; then
  id="$(date +%s)-$session-$$"
  printf 'args=%s\n' "$args" > "$dir/requests/$id.part"
  mv -f "$dir/requests/$id.part" "$dir/requests/$id" 2>/dev/null ||
    printf 'args=%s\n' "$args" > "$dir/requests/$id"
  printf '%s\n' "$id" > "$dir/pending/$session"
  printf 'requested %s  (args: %s)\n' "$id" "${args:-<none>}"
else
  id=$(cat "$dir/pending/$session" 2>/dev/null || true)
  if [ -z "$id" ]; then
    echo "check-agent-run: keine offene Anfrage dieser Session" >&2
    exit 2
  fi
fi

waited=0
while [ "$waited" -lt "$wait_secs" ]; do
  [ -f "$dir/results/$id" ] && report "$id"

  if [ ! -f "$dir/requests/$id" ]; then
    # The watcher removes a request only after the result is in place, so this
    # is either that instant or a dropped request. Give it one moment, then say
    # so instead of waiting out the timeout.
    sleep 2
    [ -f "$dir/results/$id" ] && report "$id"
    echo "REQUEST-GONE: $id ist weder in der Warteschlange noch als Ergebnis vorhanden." >&2
    echo "Der Lauf wurde verworfen (Watcher-Neustart oder zu alt). Neu anfordern, nicht warten." >&2
    exit 5
  fi

  sleep 2
  waited=$(( waited + 2 ))
done

echo "PENDING: läuft noch (${waited}s). Erneut aufrufen mit: ./scripts/check-agent-run.sh --poll"
exit 4

#!/bin/sh
# Weiterleitung auf die eine Fassung in den dotfiles.
#
# Watcher und Runner tragen kein Projektwissen — sie suchen ihre Wurzel selbst
# und rufen das `scripts/check.sh` des Repos. Als Kopie in jedem Repo sind sie
# trotzdem auseinandergelaufen: eine Verbesserung landete in einem Repo, die
# anderen zwei liefen weiter mit der älteren Fassung, und weil die Kopien ihre
# Protokollnummer nicht anhoben, meldete nichts den Rückstand.
#
# Der Ort der dotfiles ist nicht festschreibbar, weil er je Sicht anders heißt:
# auf dem Rechner liegen sie neben den Projektordnern, in der Sandbox eines
# Agenten als Geschwister der eingebundenen Repos, und dort ist der echte Pfad
# gar nicht sichtbar. Also wird aufwärts gesucht. CHECK_AGENT_HOME sticht die
# Suche, falls die dotfiles einmal woanders liegen.
set -e
target=check-agent-run.sh

if [ -n "${CHECK_AGENT_HOME:-}" ] && [ -x "$CHECK_AGENT_HOME/bin/$target" ]; then
  exec "$CHECK_AGENT_HOME/bin/$target" "$@"
fi

dir=$(git rev-parse --show-toplevel 2>/dev/null) || dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
while [ "$dir" != "/" ]; do
  if [ -x "$dir/dotfiles/bin/$target" ]; then
    exec "$dir/dotfiles/bin/$target" "$@"
  fi
  dir=$(dirname -- "$dir")
done

# 127 und nicht einer der Codes des Runners: hier ist nichts geprüft worden,
# also darf das Ergebnis auch nicht wie ein Prüfergebnis aussehen.
printf '%s: dotfiles/bin/%s nicht gefunden.\n' "${0##*/}" "$target" >&2
printf 'Der Ordner dotfiles muss erreichbar sein, oder CHECK_AGENT_HOME zeigt auf ihn.\n' >&2
exit 127

#!/bin/sh
set -eu

# Routine Rust validation for the World 01 workspace.
#
#   ./scripts/check.sh            formatting, legacy namespace, compile check
#   ./scripts/check.sh --tests    additionally runs the simulation-side tests
#
# The default path stays cheap enough to run after every edit. Tests are opt-in
# because building test binaries adds another Cargo artifact family; run them
# before and after a refactor, not after every keystroke.

run_tests=0
for argument in "$@"; do
  case "$argument" in
    --tests) run_tests=1 ;;
    *)
      echo "usage: $0 [--tests]" >&2
      exit 2
      ;;
  esac
done

legacy_namespace='game''01'
if git grep -I --ignore-case --line-number "$legacy_namespace" -- .; then
  echo "error: legacy project namespace found; use world01 instead" >&2
  exit 1
fi

cargo fmt --all --check

# Separate invocations keep the headless server's feature graph independent.
cargo check --quiet --package world01-server
cargo check --quiet --package world01-client --features dev

if [ "$run_tests" -eq 1 ]; then
  # Library crates carry the simulation, content and protocol behaviour. The
  # two binaries are checked above and hold a single test between them.
  cargo test --quiet \
    --package world01-configs \
    --package world01-world-data \
    --package world01-design \
    --package world01-content \
    --package world01-network \
    --package world01-simulation
fi

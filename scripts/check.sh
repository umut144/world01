#!/bin/sh
set -eu

# Routine Rust validation for the World 01 workspace.
#
#   ./scripts/check.sh                 formatting, legacy namespace, compile check
#   ./scripts/check.sh --tests         additionally runs the library-crate tests
#   ./scripts/check.sh --apps-tests    additionally runs the two binaries' own tests
#
# The default path stays cheap enough to run after every edit. Both test flags
# are opt-in and independent, because building test binaries adds another Cargo
# artifact family per flag: run --tests after a change inside a library crate,
# --apps-tests after a change to apps/server or apps/client, both after either
# touches the other (a protocol or replication change, for instance).
#
# --apps-tests is its own flag rather than folded into --tests because the two
# binaries' tests build real Bevy Apps with the network and session plugins
# wired in, which costs more than the library suite and is only worth paying
# when session.rs or main.rs themselves changed.

run_tests=0
run_apps_tests=0
for argument in "$@"; do
  case "$argument" in
    --tests) run_tests=1 ;;
    --apps-tests) run_apps_tests=1 ;;
    *)
      echo "usage: $0 [--tests] [--apps-tests]" >&2
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
  # Library crates carry the simulation, content and protocol behaviour.
  cargo test --quiet \
    --package world01-configs \
    --package world01-world-data \
    --package world01-design \
    --package world01-content \
    --package world01-network \
    --package world01-simulation
fi

if [ "$run_apps_tests" -eq 1 ]; then
  # cargo check above never builds the test target, so #[cfg(test)] code in
  # either binary is only ever compiled - let alone run - here.
  cargo test --quiet --package world01-server
  cargo test --quiet --package world01-client --features dev
fi

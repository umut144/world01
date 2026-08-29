#!/bin/sh
set -eu

legacy_namespace='game''01'
if git grep -I --ignore-case --line-number "$legacy_namespace" -- .; then
  echo "error: legacy project namespace found; use world01 instead" >&2
  exit 1
fi

# Separate invocations keep the headless server's feature graph independent.
cargo check --quiet --package world01-server
cargo check --quiet --package world01-client --features dev

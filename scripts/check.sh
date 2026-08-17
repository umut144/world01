#!/bin/sh
set -eu

# Separate invocations keep the headless server's feature graph independent.
cargo check --quiet --package game01-server
cargo check --quiet --package game01-client --features dev

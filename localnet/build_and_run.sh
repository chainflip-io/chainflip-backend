#!/bin/bash
# Does a cargo build and then recreates the localnet using the new binaries.
# Usage: ./localnet/build_and_run.sh

cargo build && ./localnet/recreate.sh -d && (cd bouncer && ./setup_for_test.sh)

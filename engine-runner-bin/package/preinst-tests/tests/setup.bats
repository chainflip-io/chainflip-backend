#!/usr/bin/env bats
CF_ROOT=/tmp/chainflip
setup() {
    # Load our mocks
    source mocks/mock_commands.sh
    mkdir -p $CF_ROOT/data.db
}

teardown() {
    # Cleanup any artifacts or reset environment variables
    unset VERSION_OUTPUT
    unset SERVICE_STATUS
    rm -rf $CF_ROOT
}

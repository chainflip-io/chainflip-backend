#!/usr/bin/env bats

source 'tests/setup.bats'

@test "Check when chainflip-node service is active" {
    # Set the mock to behave as if the service is active
    SERVICE_STATUS="active"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service)
    # Assert on the result
    [ "$result" = "chainflip-node stopped" ]
}

@test "Check when chainflip-node service is inactive" {
    # Set the mock to behave as if the service is inactive
    SERVICE_STATUS="inactive"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service 2>&1)
    # Assert on the result
    [ "$result" = "chainflip-node is already stopped" ]
}

@test "Preinstall script doesn't run on fresh install" {
    source ../perseverance/preinst

    result=$(check_upgrade)

    [ "$result" = "chainflip-node: Fresh install detected, skipping migration" ]
}

@test "Preinstall script runs on upgrade" {
    source ../perseverance/preinst

    result=$(check_upgrade "upgrade")

    [ "$result" = "chainflip-node: Upgrade detected, migrating" ]
}

@test "Migration runs if local minor version doesn't match target" {
    VERSION_OUTPUT="chainflip-node 0.8.1-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)

    [ "$result" = "chainflip-node: Detected older version, migrating" ]
}

@test "Migration doesn't run if local patch version matchs target" {
    VERSION_OUTPUT="chainflip-node 0.9.1-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)
    echo $result
    [ "$result" = "chainflip-node: skipping migration" ]
}

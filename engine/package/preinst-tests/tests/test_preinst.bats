#!/usr/bin/env bats

source 'tests/setup.bats'

@test "Check when chainflip-engine service is active" {
    # Set the mock to behave as if the service is active
    SERVICE_STATUS="active"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service)
    # Assert on the result
    [ "$result" = "chainflip-engine stopped" ]
}

@test "Check when chainflip-engine service is inactive" {
    # Set the mock to behave as if the service is inactive
    SERVICE_STATUS="inactive"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service 2>&1)
    # Assert on the result
    [ "$result" = "chainflip-engine is already stopped" ]
}

@test "Preinstall script doesn't run on fresh install" {
    source ../perseverance/preinst

    result=$(check_upgrade)

    [ "$result" = "chainflip-engine: Fresh install detected, skipping migration" ]
}

@test "Preinstall script runs on upgrade" {
    source ../perseverance/preinst

    result=$(check_upgrade "upgrade")

    [ "$result" = "chainflip-engine: Upgrade detected, migrating" ]
}

@test "Migration runs if local minor version doesn't match target" {
    VERSION_OUTPUT="chainflip-engine 0.8.1-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)

    [ "$result" = "chainflip-engine: Detected older version, migrating" ]
}

@test "Migration doesn't run if local minor version matchs target" {
    VERSION_OUTPUT="chainflip-engine 0.9.1-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)
    echo $result
    [ "$result" = "chainflip-engine: skipping migration" ]
}

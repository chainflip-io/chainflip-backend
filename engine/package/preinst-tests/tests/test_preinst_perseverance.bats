#!/usr/bin/env bats

source 'tests/setup.bats'

@test "Check when chainflip-engine service is active" {
    # Set the mock to behave as if the service is active
    SERVICE_STATUS="active"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service "chainflip-engine")
    # Assert on the result
    [ "$result" = "chainflip-engine stopped" ]
}

@test "Check when chainflip-engine service is inactive" {
    # Set the mock to behave as if the service is inactive
    SERVICE_STATUS="inactive"

    source ../perseverance/preinst

    # Run the function
    result=$(stop_service "chainflip-engine" 2>&1)
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

@test "Migration runs if current version is less than target" {
    VERSION_OUTPUT="chainflip-engine 0.9.3-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)

    [ "$result" = "Current version is less than target version, migrating" ]
}

@test "Migration doesn't run if current version is greater than target" {
    VERSION_OUTPUT="chainflip-engine 0.11.0-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)
    echo $result
    [ "$result" = "Current version is greater than target version, skipping migration" ]
}

@test "Migration doesn't run if current version is equal to target" {
    VERSION_OUTPUT="chainflip-engine 0.10.0-7989b2cb5e4"

    source ../perseverance/preinst
    result=$(check_version)

    [ "$result" = "Both versions are equal. Skipping migration" ]
}

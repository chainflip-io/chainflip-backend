#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes 1 mandatory argument, and 2 optional arguments.
// Arguments:
// 1. Path to the runtime wasm file
// 2. Optional: A JSON string representing the semver restriction for the upgrade. If not provided, the upgrade will not be restricted by semver.
// 3. Optional: A number representing the percentage of nodes that must be upgraded before the upgrade will be allowed to proceed. If not provided, the upgrade will not be restricted by the number of nodes that have upgraded.
//
// For example: ./commands/submit_runtime_upgrade.ts /path/to/state_chain_runtime.compact.compressed.wasm '{"major": 1, "minor": 2, "patch": 3}' 50

import { submitRuntimeUpgrade } from '../shared/submit_runtime_upgrade';
import { runWithTimeout } from '../shared/utils';

async function main() {
    const wasmPath = process.argv[2];

    const arg3 = process.argv[3].trim();
    const semverRestriction = arg3 ? JSON.parse(arg3) : undefined;

    const arg4 = process.argv[4].trim();
    const percentNodesUpgraded = arg4 ? Number(arg4) : undefined;

    await submitRuntimeUpgrade(wasmPath, semverRestriction, percentNodesUpgraded);
    process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
    console.error(error);
    process.exit(-1);
});

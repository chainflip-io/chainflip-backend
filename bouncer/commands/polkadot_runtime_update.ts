#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
// Updates the polkadot network to a new spec version via a runtime update with no other changes to the code.
// The new spec version will be +1 from the current polkadot spec version.
// The first time the script is run, it will be faster because it uses a precompiled runtime.
// Subsequent runs will be slower because it needs to compile the runtime.

import {
  bumpAndBuildPolkadotRuntime,
  pushPolkadotRuntimeUpdate,
} from '../tests/polkadot_runtime_update';
import { runWithTimeoutAndExit } from '../shared/utils';
import { getNetworkRuntimeVersion } from '../shared/utils/spec_version';
import { globalLogger } from '../shared/utils/logger';

async function main(): Promise<void> {
  // Bump the spec version
  const [wasmPath, expectedSpecVersion] = await bumpAndBuildPolkadotRuntime(globalLogger);

  // Submit the runtime update
  await pushPolkadotRuntimeUpdate(globalLogger, wasmPath);

  // Check the polkadot spec version has changed
  const postUpgradeSpecVersion = await getNetworkRuntimeVersion(
    globalLogger,
    'http://127.0.0.1:9947',
  );
  if (postUpgradeSpecVersion.specVersion !== expectedSpecVersion) {
    throw new Error(
      `Polkadot runtime update failed. Currently at version ${postUpgradeSpecVersion.specVersion}, expected to be at ${expectedSpecVersion}`,
    );
  }
}

await runWithTimeoutAndExit(main(), 400);

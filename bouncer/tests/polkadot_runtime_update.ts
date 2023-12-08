#!/usr/bin/env -S pnpm tsx
import { Assets, Asset } from '@chainflip-io/cli';
import { runWithTimeout, sleep } from '../shared/utils';
import { getCurrentRuntimeVersion } from '../shared/utils/bump_spec_version';
import { testSwap } from '../shared/swapping';
import {
  bumpAndBuildPolkadotRuntime,
  isRuntimeUpdatePushed,
  pushPolkadotRuntimeUpgrade,
} from '../shared/polkadot_runtime_update';

const POLKADOT_ENDPOINT_PORT = 9947;

let swapsComplete = 0;
let swapsStarted = 0;

async function randomPolkadotSwap(): Promise<void> {
  const assets: Asset[] = [Assets.BTC, Assets.ETH, Assets.USDC, Assets.FLIP];
  const randomAsset = assets[Math.floor(Math.random() * assets.length)];

  let sourceAsset: Asset;
  let destAsset: Asset;

  if (Math.random() < 0.5) {
    sourceAsset = Assets.DOT;
    destAsset = randomAsset;
  } else {
    sourceAsset = randomAsset;
    destAsset = Assets.DOT;
  }

  await testSwap(sourceAsset, destAsset, undefined, undefined, undefined, undefined, false);
  swapsComplete++;
  console.log(`Swap complete: (${swapsComplete}/${swapsStarted})`);
}

async function doPolkadotSwaps(): Promise<void> {
  const startSwapInterval = 2000;
  console.log(`Running polkadot swaps, new random swap every ${startSwapInterval}ms`);
  while (!isRuntimeUpdatePushed()) {
    randomPolkadotSwap();
    swapsStarted++;
    await sleep(startSwapInterval);
  }
  console.log(`Stopping polkadot swaps, ${swapsComplete}/${swapsStarted} swaps complete.`);

  // Wait for all of the swaps to complete
  while (swapsComplete < swapsStarted) {
    await sleep(1000);
  }
  console.log(`All ${swapsComplete} swaps complete`);
}

async function main(): Promise<void> {
  const [wasmPath, expectedSpecVersion] = await bumpAndBuildPolkadotRuntime();

  // Start some swaps
  const swapping = doPolkadotSwaps();
  console.log('Waiting for swaps to start...');
  while (swapsComplete === 0) {
    await sleep(1000);
  }

  // Submit the runtime upgrade
  await pushPolkadotRuntimeUpgrade(wasmPath);

  // Check the polkadot spec version has changed
  const postUpgradeSpecVersion = await getCurrentRuntimeVersion(POLKADOT_ENDPOINT_PORT);
  if (postUpgradeSpecVersion.specVersion !== expectedSpecVersion) {
    throw new Error(
      `Polkadot runtime upgrade failed. Currently at version ${postUpgradeSpecVersion.specVersion}, expected to be at ${expectedSpecVersion}`,
    );
  }

  // Wait for all of the swaps to complete
  console.log('Waiting for swaps to complete...');
  await swapping;

  process.exit(0);
}

runWithTimeout(main(), 1230000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

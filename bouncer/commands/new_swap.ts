#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("Btc", "Eth", "Dot" or "Usdc")
// Argument 2 is the destination currency ("Btc", "Eth", "Dot" or "Usdc")
// Argument 3 is the destination address
// For example: ./commands/new_swap.ts Dot Btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX

import { InternalAsset as Asset } from '@chainflip/cli';
import { runWithTimeout } from '../shared/utils';
import { newSwap } from '../shared/new_swap';

async function newSwapCommand() {
  const sourceAsset = parseAssetString(process.argv[2]);
  const destAsset = parseAssetString(process.argv[3]);
  const destAddress = process.argv[4];

  console.log(`Requesting swap ${sourceAsset} -> ${destAsset}`);

  await newSwap(sourceAsset, destAsset, destAddress);

  process.exit(0);
}

runWithTimeout(newSwapCommand(), 60000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

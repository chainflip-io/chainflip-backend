#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("btc", "eth", "dot" or "usdc")
// Argument 2 is the destination currency ("btc", "eth", "dot" or "usdc")
// Argument 3 is the destination address
// Argument 4 is the broker fee in basis points
// For example: ./commands/new_swap.ts dot btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX 100

import { Asset } from '@chainflip-io/cli';
import { runWithTimeout } from '../shared/utils';
import { newSwap } from '../shared/new_swap';

async function newSwapCommand() {
  const sourceAsset = process.argv[2].toUpperCase() as Asset;
  const destAsset = process.argv[3].toUpperCase() as Asset;
  const destAddress = process.argv[4];
  const fee = parseFloat(process.argv[5]);

  console.log(`Requesting swap ${sourceAsset} -> ${destAsset}`);

  await newSwap(sourceAsset, destAsset, destAddress, fee);

  process.exit(0);
}

runWithTimeout(newSwapCommand(), 60000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

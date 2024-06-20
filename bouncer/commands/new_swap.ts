#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("Btc", "Eth", "Dot" or "Usdc")
// Argument 2 is the destination currency ("Btc", "Eth", "Dot" or "Usdc")
// Argument 3 is the destination address
// Argument 4 (optional) is the max boost fee bps (default: 0 (no boosting))
// For example: ./commands/new_swap.ts Dot Btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX

import { parseAssetString, executeWithTimeout } from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';

async function newSwapCommand() {
  const sourceAsset = parseAssetString(process.argv[2]);
  const destAsset = parseAssetString(process.argv[3]);
  const destAddress = process.argv[4];
  const maxBoostFeeBps = Number(process.argv[5] || 0);

  console.log(`Requesting swap ${sourceAsset} -> ${destAsset}`);

  await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    undefined,
    undefined,
    undefined,
    true,
    maxBoostFeeBps,
  );
}

await executeWithTimeout(newSwapCommand(), 60);

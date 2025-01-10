#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Dot balance of the address provided as the first argument.
//
// For example: ./commands/get_hubasset_balance.ts HubUsdc 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci
// might print: 1.2

import { HubAsset, runWithTimeoutAndExit } from '../shared/utils';
import { getHubAssetBalance } from '../shared/get_hub_balance';

function stringToHubAsset(asset: string): HubAsset {
  if (asset === 'HubUsdc' || asset === 'HubUsdt') {
    return asset;
  } else {
      throw new Error(`Unsupported Assethub asset: ${asset}`);
  }
}

async function getHubAssetBalanceCommand(asset: string, address: string) {
  console.log(await getHubAssetBalance(stringToHubAsset(asset), address));
}

const asset = process.argv[2];
const address = process.argv[3] ?? '0';
await runWithTimeoutAndExit(getHubAssetBalanceCommand(asset, address), 5);

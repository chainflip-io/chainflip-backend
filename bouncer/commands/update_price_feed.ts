#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two argument.
// It will update the on-chain price feed for that asset.
//
// For example: ./commands/update_price_feed.ts BTC 123456

import { Asset } from '@chainflip/cli';
import { updateEvmPriceFeed } from '../shared/update_price_feed';
import { runWithTimeoutAndExit } from '../shared/utils';

export async function updatePriceFeed(asset: string, price: string) {
  await updateEvmPriceFeed(asset as Asset, price);
}

const asset = process.argv[2];
const price = process.argv[3].trim();
await runWithTimeoutAndExit(updatePriceFeed(asset, price), 20);

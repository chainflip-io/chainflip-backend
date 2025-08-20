#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// The first argument is the asset to update the price feed for, or "ALL" to update all default price feeds.
// The second argument is the price (optional when asset is "ALL").
//
// For example: ./commands/update_price_feeds.ts BTC 123456
// Or: ./commands/update_price_feeds.ts ALL

import { Asset } from '@chainflip/cli';
import { updatePriceFeed, updateDefaultPriceFeeds } from '../shared/update_price_feed';
import { runWithTimeoutAndExit } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

export async function updatePriceFeeds(asset: string, price?: string) {
  if (asset.toUpperCase() === 'ALL') {
    await updateDefaultPriceFeeds(globalLogger);
  } else {
    if (price === undefined) {
      throw new Error('Price argument is required to set the price feed of a specific asset.');
    }
    await updatePriceFeed(globalLogger, 'Ethereum', asset as Asset, price);
    await updatePriceFeed(globalLogger, 'Solana', asset as Asset, price);
  }
}

const asset = process.argv[2];
const price = process.argv[3]?.trim();

await runWithTimeoutAndExit(updatePriceFeeds(asset, price), 100);

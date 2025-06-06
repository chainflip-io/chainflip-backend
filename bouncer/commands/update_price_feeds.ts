#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two argument.
// It will update the on-chain price feed for that asset.
//
// For example: ./commands/update_price_feeds.ts BTC 123456

import { Asset } from '@chainflip/cli';
import { updatePriceFeed } from '../shared/update_price_feed';
import { runWithTimeoutAndExit } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

export async function updatePriceFeeds(asset: string, price: string) {
  await updatePriceFeed(globalLogger, 'Ethereum', asset as Asset, price);
  await updatePriceFeed(globalLogger, 'Solana', asset as Asset, price);
}

const asset = process.argv[2];
const price = process.argv[3].trim();
await runWithTimeoutAndExit(updatePriceFeeds(asset, price), 20);

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// The first argument is the asset to update the price feed for, or "ALL" to update all default price feeds.
// The second argument is the price (optional when asset is "ALL").
// The third argument is the source chain to update (optional; defaults to every configured source
// for that asset).
//
// For example: ./commands/update_price_feeds.ts BTC 123456
// Or: ./commands/update_price_feeds.ts BTC 123456 Bsc
// Or: ./commands/update_price_feeds.ts TRX 0.42
// Or: ./commands/update_price_feeds.ts ALL

import { AssetSymbol as Asset } from '@chainflip/utils/chainflip';
import {
  getPriceFeedChains,
  updatePriceFeed,
  updateDefaultPriceFeeds,
} from 'shared/update_price_feed';
import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';

const PRICE_FEED_SOURCE_CHAINS = ['Ethereum', 'Arbitrum', 'Bsc'] as const;

type PriceFeedSourceChain = (typeof PRICE_FEED_SOURCE_CHAINS)[number];

function normalizePriceFeedSourceChain(chain: string): PriceFeedSourceChain {
  const normalized = PRICE_FEED_SOURCE_CHAINS.find(
    (sourceChain) => sourceChain.toLowerCase() === chain.toLowerCase(),
  );

  if (!normalized) {
    throw new Error(
      `Unsupported price feed source chain: ${chain}. Supported chains: ${PRICE_FEED_SOURCE_CHAINS.join(', ')}`,
    );
  }

  return normalized;
}

export async function updatePriceFeeds(asset: string | undefined, price?: string, chain?: string) {
  if (asset === undefined) {
    throw new Error('Asset argument is required. Pass an asset symbol or ALL.');
  }

  if (asset.toUpperCase() === 'ALL') {
    if (chain !== undefined) {
      throw new Error('Chain argument is not supported when updating ALL price feeds.');
    }

    await updateDefaultPriceFeeds(globalLogger);
  } else {
    if (price === undefined) {
      throw new Error('Price argument is required to set the price feed of a specific asset.');
    }

    const normalizedAsset = asset.toUpperCase() as Asset;
    const sourceChains = chain
      ? [normalizePriceFeedSourceChain(chain)]
      : getPriceFeedChains(normalizedAsset);

    const supportedSourceChains = getPriceFeedChains(normalizedAsset);
    const unsupportedSourceChain = sourceChains.find(
      (sourceChain) => !supportedSourceChains.includes(sourceChain),
    );

    if (unsupportedSourceChain) {
      throw new Error(
        `${normalizedAsset} price feed is not configured on ${unsupportedSourceChain}. Configured source chains: ${supportedSourceChains.join(', ')}`,
      );
    }

    await Promise.all(
      sourceChains.map((sourceChain) =>
        updatePriceFeed(globalLogger, sourceChain, normalizedAsset, price),
      ),
    );
  }
}

const asset = process.argv[2];
const price = process.argv[3]?.trim();
const chain = process.argv[4]?.trim();

await runWithTimeoutAndExit(updatePriceFeeds(asset, price, chain), 100);

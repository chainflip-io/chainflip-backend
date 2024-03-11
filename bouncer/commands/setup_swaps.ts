#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip/cli';
import { runWithTimeout } from '../shared/utils';
import { createLpPool } from '../shared/create_lp_pool';
import { provideLiquidity } from '../shared/provide_liquidity';
import { rangeOrder } from '../shared/range_order';

const deposits = new Map<Asset, number>([
  ['DOT', 10000],
  ['ETH', 100],
  ['ARBETH', 100],
  ['BTC', 10],
  ['USDC', 1000000],
  ['ARBUSDC', 100000],
  ['FLIP', 10000],
  ['SOL', 100],
  ['SOLUSDC', 100000],
]);

const price = new Map<Asset, number>([
  ['DOT', 10],
  ['ETH', 1000],
  ['ARBETH', 1000],
  ['BTC', 10000],
  ['USDC', 1],
  ['ARBUSDC', 1],
  ['FLIP', 10],
  ['SOL', 100],
  ['SOLUSDC', 1],
]);

async function main(): Promise<void> {
  await cryptoWaitReady();

  await Promise.all([
    createLpPool('ETH', price.get('ETH')!),
    createLpPool('DOT', price.get('DOT')!),
    createLpPool('BTC', price.get('BTC')!),
    createLpPool('FLIP', price.get('FLIP')!),
    createLpPool('SOL', price.get('SOL')!),
    createLpPool('SOLUSDC', price.get('SOLUSDC')!),
    // // createLpPool('ARBETH', price.get('ARBETH')!),
    // // createLpPool('ARBUSDC', price.get('ARBUSDC')!),
  ]);

  console.log('Providing liquidity');

  await Promise.all([
    provideLiquidity('USDC', deposits.get('USDC')!),
    provideLiquidity('ETH', deposits.get('ETH')!),
    provideLiquidity('DOT', deposits.get('DOT')!),
    provideLiquidity('BTC', deposits.get('BTC')!),
    provideLiquidity('FLIP', deposits.get('FLIP')!),
    provideLiquidity('SOL', deposits.get('SOL')!),
    // provideLiquidity('SOLUSDC', deposits.get('SOLUSDC')!), // This won't work yet until we witness SOLUSDC
    // // provideLiquidity('ARBETH', deposits.get('ARBETH')!),
    // // provideLiquidity('ARBUSDC', deposits.get('ARBUSDC')!),
  ]);

  await Promise.all([
    rangeOrder('ETH', deposits.get('ETH')! * 0.9999),
    rangeOrder('DOT', deposits.get('DOT')! * 0.9999),
    rangeOrder('BTC', deposits.get('BTC')! * 0.9999),
    rangeOrder('FLIP', deposits.get('FLIP')! * 0.9999),
    rangeOrder('SOL', deposits.get('SOL')! * 0.9999),
    // rangeOrder('SOLUSDC', deposits.get('SOLUSDC')! * 0.9999), // This won't work yet until we witness SOLUSDC
    // // rangeOrder('ARBETH', deposits.get('ARBETH')! * 0.9999),
    // // rangeOrder('ARBUSDC', deposits.get('ARBUSDC')! * 0.9999),
  ]);
  console.log('=== Swaps Setup completed ===');

  process.exit(0);
}

runWithTimeout(main(), 2400000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

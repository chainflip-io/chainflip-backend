#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip/cli';
import { getEvmContractAddress, runWithTimeout, sleep } from '../shared/utils';
import { createLpPool } from '../shared/create_lp_pool';
import { provideLiquidity } from '../shared/provide_liquidity';
import { rangeOrder } from '../shared/range_order';
import { getEvmNativeBalance } from '../shared/get_evm_native_balance';
import { getErc20Balance } from '../shared/get_erc20_balance';

const deposits = new Map<Asset, number>([
  ['DOT', 10000],
  ['ETH', 100],
  ['ARBETH', 100],
  ['BTC', 10],
  ['USDC', 1000000],
  ['ARBUSDC', 100000],
  ['FLIP', 10000],
]);

const price = new Map<Asset, number>([
  ['DOT', 10],
  ['ETH', 1000],
  ['ARBETH', 1000],
  ['BTC', 10000],
  ['USDC', 1],
  ['ARBUSDC', 1],
  ['FLIP', 10],
]);

async function main(): Promise<void> {
  await cryptoWaitReady();

  await Promise.all([
    createLpPool('ETH', price.get('ETH')!),
    createLpPool('DOT', price.get('DOT')!),
    createLpPool('BTC', price.get('BTC')!),
    createLpPool('FLIP', price.get('FLIP')!),
    createLpPool('ARBETH', price.get('ARBETH')!),
    createLpPool('ARBUSDC', price.get('ARBUSDC')!),
  ]);

  await Promise.all([
    provideLiquidity('USDC', deposits.get('USDC')!),
    provideLiquidity('ETH', deposits.get('ETH')!),
    provideLiquidity('DOT', deposits.get('DOT')!),
    provideLiquidity('BTC', deposits.get('BTC')!),
    provideLiquidity('FLIP', deposits.get('FLIP')!),
    provideLiquidity('ARBETH', deposits.get('ARBETH')!),
    provideLiquidity('ARBUSDC', deposits.get('ARBUSDC')!),
  ]);

  // also fund the boost account
  await Promise.all([
    provideLiquidity('USDC', deposits.get('USDC')!, false, '//LP_BOOST'),
    provideLiquidity('ETH', deposits.get('ETH')!, false, '//LP_BOOST'),
    provideLiquidity('DOT', deposits.get('DOT')!, false, '//LP_BOOST'),
    provideLiquidity('BTC', deposits.get('BTC')!, false, '//LP_BOOST'),
    provideLiquidity('FLIP', deposits.get('FLIP')!, false, '//LP_BOOST'),
    provideLiquidity('ARBETH', deposits.get('ARBETH')!, false, '//LP_BOOST'),
    provideLiquidity('ARBUSDC', deposits.get('ARBUSDC')!, false, '//LP_BOOST'),
  ]);

  await Promise.all([
    rangeOrder('ETH', deposits.get('ETH')! * 0.9999),
    rangeOrder('DOT', deposits.get('DOT')! * 0.9999),
    rangeOrder('BTC', deposits.get('BTC')! * 0.9999),
    rangeOrder('FLIP', deposits.get('FLIP')! * 0.9999),
    rangeOrder('ARBETH', deposits.get('ARBETH')! * 0.9999),
    rangeOrder('ARBUSDC', deposits.get('ARBUSDC')! * 0.9999),
  ]);

  console.log('=== Swaps Setup completed ===');
  process.exit(0);
}

runWithTimeout(main(), 2400000).catch((error) => {
  console.error(error);
  process.exit(-1);
});

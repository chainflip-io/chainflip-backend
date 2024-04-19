import { cryptoWaitReady } from '@polkadot/util-crypto';
import { InternalAsset as Asset } from '@chainflip/cli';
import { createLpPool } from '../shared/create_lp_pool';
import { provideLiquidity } from '../shared/provide_liquidity';
import { rangeOrder } from '../shared/range_order';

const deposits = new Map<Asset, number>([
  ['Dot', 10000],
  ['Eth', 100],
  ['ArbEth', 100],
  ['Btc', 10],
  ['Usdc', 1000000],
  ['ArbUsdc', 100000],
  ['Usdt', 100000],
  ['Flip', 10000],
  ['Sol', 100],
  ['SolUsdc', 100000],
]);

const price = new Map<Asset, number>([
  ['Dot', 10],
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 10000],
  ['Usdc', 1],
  ['Usdt', 1],
  ['ArbUsdc', 1],
  ['Flip', 10],
  ['Sol', 100],
  ['SolUsdc', 1],
]);

export async function setupSwaps(): Promise<void> {
  await cryptoWaitReady();

  await Promise.all([
    createLpPool('Eth', price.get('Eth')!),
    createLpPool('Dot', price.get('Dot')!),
    createLpPool('Btc', price.get('Btc')!),
    createLpPool('Flip', price.get('Flip')!),
    createLpPool('Usdt', price.get('Usdt')!),
    createLpPool('ArbEth', price.get('ArbEth')!),
    createLpPool('ArbUsdc', price.get('ArbUsdc')!),
    // createLpPool('Sol', price.get('Sol')!),
    // createLpPool('SolUsdc', price.get('SolUsdc')!),
  ]);

  await Promise.all([
    provideLiquidity('Usdc', deposits.get('Usdc')!),
    provideLiquidity('Eth', deposits.get('Eth')!),
    provideLiquidity('Dot', deposits.get('Dot')!),
    provideLiquidity('Btc', deposits.get('Btc')!),
    provideLiquidity('Flip', deposits.get('Flip')!),
    provideLiquidity('Usdt', deposits.get('Usdt')!),
    provideLiquidity('ArbEth', deposits.get('ArbEth')!),
    provideLiquidity('ArbUsdc', deposits.get('ArbUsdc')!),
    // provideLiquidity('Sol', deposits.get('Sol')!),
    // provideLiquidity('SolUsdc', deposits.get('SolUsdc')!),
  ]);

  // also fund the boost account
  await Promise.all([
    provideLiquidity('Usdc', deposits.get('Usdc')!, false, '//LP_BOOST'),
    provideLiquidity('Eth', deposits.get('Eth')!, false, '//LP_BOOST'),
    provideLiquidity('Dot', deposits.get('Dot')!, false, '//LP_BOOST'),
    provideLiquidity('Btc', deposits.get('Btc')!, false, '//LP_BOOST'),
    provideLiquidity('Flip', deposits.get('Flip')!, false, '//LP_BOOST'),
    provideLiquidity('Usdt', deposits.get('Usdt')!, false, '//LP_BOOST'),
    provideLiquidity('ArbEth', deposits.get('ArbEth')!, false, '//LP_BOOST'),
    provideLiquidity('ArbUsdc', deposits.get('ArbUsdc')!, false, '//LP_BOOST'),
    // provideLiquidity('Sol', deposits.get('Sol')!, false, '//LP_BOOST'),
    // provideLiquidity('SolUsdc', deposits.get('SolUsdc')!, false, '//LP_BOOST'),
  ]);

  await Promise.all([
    rangeOrder('Eth', deposits.get('Eth')! * 0.9999),
    rangeOrder('Dot', deposits.get('Dot')! * 0.9999),
    rangeOrder('Btc', deposits.get('Btc')! * 0.9999),
    rangeOrder('Flip', deposits.get('Flip')! * 0.9999),
    rangeOrder('Usdt', deposits.get('Usdt')! * 0.9999),
    rangeOrder('ArbEth', deposits.get('ArbEth')! * 0.9999),
    rangeOrder('ArbUsdc', deposits.get('ArbUsdc')! * 0.9999),
    // rangeOrder('Sol', deposits.get('Sol')! * 0.9999),
    // rangeOrder('SolUsdc', deposits.get('SolUsdc')! * 0.9999),
  ]);

  console.log('=== Swaps Setup completed ===');
  process.exit(0);
}

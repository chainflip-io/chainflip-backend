import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from './deposit_liquidity';
import { rangeOrder } from '../shared/range_order';
import { Asset } from './utils';

const deposits = new Map<Asset, number>([
  ['Dot', 100000],
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 100],
  ['Usdc', 10000000],
  ['ArbUsdc', 1000000],
  ['Usdt', 1000000],
  ['Flip', 100000],
  ['Sol', 1000],
  ['SolUsdc', 1000000],
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
  console.log('=== Setting up for swaps ===');

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

  console.log('LP Pools created');

  await Promise.all([
    depositLiquidity('Usdc', deposits.get('Usdc')!),
    depositLiquidity('Eth', deposits.get('Eth')!),
    depositLiquidity('Dot', deposits.get('Dot')!),
    depositLiquidity('Btc', deposits.get('Btc')!),
    depositLiquidity('Flip', deposits.get('Flip')!),
    depositLiquidity('Usdt', deposits.get('Usdt')!),
    depositLiquidity('ArbEth', deposits.get('ArbEth')!),
    depositLiquidity('ArbUsdc', deposits.get('ArbUsdc')!),
    // provideLiquidity('Sol', deposits.get('Sol')!),
    // provideLiquidity('SolUsdc', deposits.get('SolUsdc')!),
  ]);

  console.log('Liquidity provided');

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

  console.log('Range orders placed');

  console.log('=== Swaps Setup completed ===');
}

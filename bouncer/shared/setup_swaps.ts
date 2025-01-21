import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from './deposit_liquidity';
import { rangeOrder } from '../shared/range_order';
import { Asset } from './utils';

export const deposits = new Map<Asset, number>([
  ['Dot', 200000],
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 100],
  ['Usdc', 10000000],
  ['ArbUsdc', 1000000],
  ['Usdt', 1000000],
  ['Flip', 100000],
  ['Sol', 1000],
  ['SolUsdc', 1000000],
  ['HubDot', 100000],
  ['HubUsdc', 250000],
  ['HubUsdt', 250000],
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
  ['HubDot', 10],
  ['HubUsdc', 1],
  ['HubUsdt', 1],
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
    createLpPool('Sol', price.get('Sol')!),
    createLpPool('SolUsdc', price.get('SolUsdc')!),
    createLpPool('HubDot', price.get('HubDot')!),
    createLpPool('HubUsdc', price.get('HubUsdc')!),
    createLpPool('HubUsdt', price.get('HubUsdt')!),
  ]);

  const lp1Deposits = Promise.all([
    depositLiquidity('Usdc', deposits.get('Usdc')!, false, '//LP_1'),
    depositLiquidity('Eth', deposits.get('Eth')!, false, '//LP_1'),
    depositLiquidity('Dot', deposits.get('Dot')!, false, '//LP_1'),
    depositLiquidity('Btc', deposits.get('Btc')!, false, '//LP_1'),
    depositLiquidity('Flip', deposits.get('Flip')!, false, '//LP_1'),
    depositLiquidity('Usdt', deposits.get('Usdt')!, false, '//LP_1'),
    depositLiquidity('ArbEth', deposits.get('ArbEth')!, false, '//LP_1'),
    depositLiquidity('ArbUsdc', deposits.get('ArbUsdc')!, false, '//LP_1'),
    depositLiquidity('Sol', deposits.get('Sol')!, false, '//LP_1'),
    depositLiquidity('SolUsdc', deposits.get('SolUsdc')!, false, '//LP_1'),
    depositLiquidity('HubDot', deposits.get('HubDot')!, false, '//LP_1'),
    depositLiquidity('HubUsdc', deposits.get('HubUsdc')!, false, '//LP_1'),
    depositLiquidity('HubUsdt', deposits.get('HubUsdt')!, false, '//LP_1'),
  ]);

  const lpApiDeposits = Promise.all([
    depositLiquidity('Usdc', 1000, false, '//LP_API'),
    depositLiquidity('Eth', 100, false, '//LP_API'),
    depositLiquidity('Dot', 2000, false, '//LP_API'),
    depositLiquidity('Btc', 10, false, '//LP_API'),
    depositLiquidity('Flip', 10000, false, '//LP_API'),
    depositLiquidity('Usdt', 1000, false, '//LP_API'),
    depositLiquidity('ArbEth', 10, false, '//LP_API'),
    depositLiquidity('ArbUsdc', 1000, false, '//LP_API'),
    depositLiquidity('Sol', 500, false, '//LP_API'),
    depositLiquidity('SolUsdc', 1000, false, '//LP_API'),
    depositLiquidity('HubDot', 2000, false, '//LP_API'),
    depositLiquidity('HubUsdc', 1000, false, '//LP_API'),
    depositLiquidity('HubUsdt', 1000, false, '//LP_API'),
  ]);

  await Promise.all([lp1Deposits, lpApiDeposits]);

  await Promise.all([
    rangeOrder('Eth', deposits.get('Eth')! * 0.9999),
    rangeOrder('Dot', deposits.get('Dot')! * 0.9999),
    rangeOrder('Btc', deposits.get('Btc')! * 0.9999),
    rangeOrder('Flip', deposits.get('Flip')! * 0.9999),
    rangeOrder('Usdt', deposits.get('Usdt')! * 0.9999),
    rangeOrder('ArbEth', deposits.get('ArbEth')! * 0.9999),
    rangeOrder('ArbUsdc', deposits.get('ArbUsdc')! * 0.9999),
    rangeOrder('Sol', deposits.get('Sol')! * 0.9999),
    rangeOrder('SolUsdc', deposits.get('SolUsdc')! * 0.9999),
    rangeOrder('HubDot', deposits.get('HubDot')! * 0.9999),
    rangeOrder('HubUsdc', deposits.get('HubUsdc')! * 0.9999),
    rangeOrder('HubUsdt', deposits.get('HubUsdt')! * 0.9999),
  ]);

  console.log('Range orders placed');

  console.log('=== Swaps Setup completed ===');
}

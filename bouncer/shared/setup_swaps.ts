import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from './deposit_liquidity';
import { rangeOrder } from '../shared/range_order';
import { Asset, getEvmRootWhaleKey } from './utils';


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

  const evmRootWhaleKey = getEvmRootWhaleKey();

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
  ]);

  const lp1Deposits = Promise.all([
    depositLiquidity('Usdc', deposits.get('Usdc')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('Eth', deposits.get('Eth')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('Dot', deposits.get('Dot')!, false, '//LP_1'),
    depositLiquidity('Btc', deposits.get('Btc')!, false, '//LP_1'),
    depositLiquidity('Flip', deposits.get('Flip')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('Usdt', deposits.get('Usdt')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('ArbEth', deposits.get('ArbEth')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('ArbUsdc', deposits.get('ArbUsdc')!, false, '//LP_1', evmRootWhaleKey),
    depositLiquidity('Sol', deposits.get('Sol')!, false, '//LP_1'),
    depositLiquidity('SolUsdc', deposits.get('SolUsdc')!, false, '//LP_1'),
  ]);

  const lpApiDeposits = Promise.all([
    depositLiquidity('Usdc', 1000, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('Eth', 100, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('Dot', 2000, false, '//LP_API'),
    depositLiquidity('Btc', 10, false, '//LP_API'),
    depositLiquidity('Flip', 10000, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('Usdt', 1000, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('ArbEth', 10, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('ArbUsdc', 1000, false, '//LP_API', evmRootWhaleKey),
    depositLiquidity('Sol', 500, false, '//LP_API'),
    depositLiquidity('SolUsdc', 1000, false, '//LP_API'),
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
  ]);

  console.log('Range orders placed');

  console.log('=== Swaps Setup completed ===');
}

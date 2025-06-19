import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { Asset } from 'shared/utils';
import { Logger } from 'shared/utils/logger';

export const deposits = new Map<Asset, number>([
  ['Dot', 20000],
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 100],
  ['Usdc', 10000000],
  ['ArbUsdc', 1000000],
  ['Usdt', 1000000],
  ['Flip', 100000],
  ['Sol', 1000],
  ['SolUsdc', 1000000],
  ['HubDot', 10000],
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

export async function setupSwaps(logger: Logger): Promise<void> {
  logger.info('Setting up for swaps');

  await Promise.all([
    createLpPool(logger, 'Eth', price.get('Eth')!),
    createLpPool(logger, 'Dot', price.get('Dot')!),
    createLpPool(logger, 'Btc', price.get('Btc')!),
    createLpPool(logger, 'Flip', price.get('Flip')!),
    createLpPool(logger, 'Usdt', price.get('Usdt')!),
    createLpPool(logger, 'ArbEth', price.get('ArbEth')!),
    createLpPool(logger, 'ArbUsdc', price.get('ArbUsdc')!),
    createLpPool(logger, 'Sol', price.get('Sol')!),
    createLpPool(logger, 'SolUsdc', price.get('SolUsdc')!),
    createLpPool(logger, 'HubDot', price.get('HubDot')!),
    createLpPool(logger, 'HubUsdc', price.get('HubUsdc')!),
    createLpPool(logger, 'HubUsdt', price.get('HubUsdt')!),
  ]);

  const lp1Deposits = Promise.all([
    depositLiquidity(logger, 'Usdc', deposits.get('Usdc')!, false, '//LP_1'),
    depositLiquidity(logger, 'Eth', deposits.get('Eth')!, false, '//LP_1'),
    depositLiquidity(logger, 'Dot', deposits.get('Dot')!, false, '//LP_1'),
    depositLiquidity(logger, 'Btc', deposits.get('Btc')!, false, '//LP_1'),
    depositLiquidity(logger, 'Flip', deposits.get('Flip')!, false, '//LP_1'),
    depositLiquidity(logger, 'Usdt', deposits.get('Usdt')!, false, '//LP_1'),
    depositLiquidity(logger, 'ArbEth', deposits.get('ArbEth')!, false, '//LP_1'),
    depositLiquidity(logger, 'ArbUsdc', deposits.get('ArbUsdc')!, false, '//LP_1'),
    depositLiquidity(logger, 'Sol', deposits.get('Sol')!, false, '//LP_1'),
    depositLiquidity(logger, 'SolUsdc', deposits.get('SolUsdc')!, false, '//LP_1'),
    depositLiquidity(logger, 'HubDot', deposits.get('HubDot')!, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdc', deposits.get('HubUsdc')!, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdt', deposits.get('HubUsdt')!, false, '//LP_1'),
  ]);

  const lpApiDeposits = Promise.all([
    depositLiquidity(logger, 'Usdc', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'Eth', 100, false, '//LP_API'),
    depositLiquidity(logger, 'Dot', 2000, false, '//LP_API'),
    depositLiquidity(logger, 'Btc', 10, false, '//LP_API'),
    depositLiquidity(logger, 'Flip', 10000, false, '//LP_API'),
    depositLiquidity(logger, 'Usdt', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'ArbEth', 10, false, '//LP_API'),
    depositLiquidity(logger, 'ArbUsdc', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'Sol', 500, false, '//LP_API'),
    depositLiquidity(logger, 'SolUsdc', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'HubDot', 2000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdc', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdt', 1000, false, '//LP_API'),
  ]);

  await Promise.all([lpApiDeposits, lp1Deposits]);

  await Promise.all([
    rangeOrder(logger, 'Eth', deposits.get('Eth')! * 0.9999),
    rangeOrder(logger, 'Dot', deposits.get('Dot')! * 0.9999),
    rangeOrder(logger, 'Btc', deposits.get('Btc')! * 0.9999),
    rangeOrder(logger, 'Flip', deposits.get('Flip')! * 0.9999),
    rangeOrder(logger, 'Usdt', deposits.get('Usdt')! * 0.9999),
    rangeOrder(logger, 'ArbEth', deposits.get('ArbEth')! * 0.9999),
    rangeOrder(logger, 'ArbUsdc', deposits.get('ArbUsdc')! * 0.9999),
    rangeOrder(logger, 'Sol', deposits.get('Sol')! * 0.9999),
    rangeOrder(logger, 'SolUsdc', deposits.get('SolUsdc')! * 0.9999),
    rangeOrder(logger, 'HubDot', deposits.get('HubDot')! * 0.9999),
    rangeOrder(logger, 'HubUsdc', deposits.get('HubUsdc')! * 0.9999),
    rangeOrder(logger, 'HubUsdt', deposits.get('HubUsdt')! * 0.9999),
  ]);

  logger.debug('Range orders placed');

  logger.info('Swaps Setup completed');
}

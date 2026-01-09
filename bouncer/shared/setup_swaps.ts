import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { Asset } from 'shared/utils';
import { ChainflipIO } from './utils/chainflip_io';

export const deposits = new Map<Asset, number>([
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

export const price = new Map<Asset, number>([
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

export async function setupSwaps<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up for swaps');

  await Promise.all([
    createLpPool(cf.logger, 'Eth', price.get('Eth')!),
    createLpPool(cf.logger, 'Btc', price.get('Btc')!),
    createLpPool(cf.logger, 'Flip', price.get('Flip')!),
    createLpPool(cf.logger, 'Usdt', price.get('Usdt')!),
    createLpPool(cf.logger, 'ArbEth', price.get('ArbEth')!),
    createLpPool(cf.logger, 'ArbUsdc', price.get('ArbUsdc')!),
    createLpPool(cf.logger, 'Sol', price.get('Sol')!),
    createLpPool(cf.logger, 'SolUsdc', price.get('SolUsdc')!),
    createLpPool(cf.logger, 'HubDot', price.get('HubDot')!),
    createLpPool(cf.logger, 'HubUsdc', price.get('HubUsdc')!),
    createLpPool(cf.logger, 'HubUsdt', price.get('HubUsdt')!),
  ]);

  const lp1Deposits = cf.all([
    (subcf) => depositLiquidity(subcf, 'Usdc', deposits.get('Usdc')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'Eth', deposits.get('Eth')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'Btc', deposits.get('Btc')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'Flip', deposits.get('Flip')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'Usdt', deposits.get('Usdt')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'ArbEth', deposits.get('ArbEth')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'ArbUsdc', deposits.get('ArbUsdc')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'Sol', deposits.get('Sol')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'SolUsdc', deposits.get('SolUsdc')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'HubDot', deposits.get('HubDot')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'HubUsdc', deposits.get('HubUsdc')!, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'HubUsdt', deposits.get('HubUsdt')!, false, '//LP_1'),
  ]);

  const lpApiDeposits = cf.all([
    (subcf) => depositLiquidity(subcf, 'Usdc', 1000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'Eth', 100, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'Btc', 10, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'Flip', 10000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'Usdt', 1000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'ArbEth', 10, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'ArbUsdc', 1000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'Sol', 500, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'SolUsdc', 1000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'HubDot', 2000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'HubUsdc', 1000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'HubUsdt', 1000, false, '//LP_API'),
  ]);

  await Promise.all([lpApiDeposits, lp1Deposits]);

  await Promise.all([
    rangeOrder(cf.logger, 'Eth', deposits.get('Eth')! * 0.9999),
    rangeOrder(cf.logger, 'Btc', deposits.get('Btc')! * 0.9999),
    rangeOrder(cf.logger, 'Flip', deposits.get('Flip')! * 0.9999),
    rangeOrder(cf.logger, 'Usdt', deposits.get('Usdt')! * 0.9999),
    rangeOrder(cf.logger, 'ArbEth', deposits.get('ArbEth')! * 0.9999),
    rangeOrder(cf.logger, 'ArbUsdc', deposits.get('ArbUsdc')! * 0.9999),
    rangeOrder(cf.logger, 'Sol', deposits.get('Sol')! * 0.9999),
    rangeOrder(cf.logger, 'SolUsdc', deposits.get('SolUsdc')! * 0.9999),
    rangeOrder(cf.logger, 'HubDot', deposits.get('HubDot')! * 0.9999),
    rangeOrder(cf.logger, 'HubUsdc', deposits.get('HubUsdc')! * 0.9999),
    rangeOrder(cf.logger, 'HubUsdt', deposits.get('HubUsdt')! * 0.9999),
  ]);

  cf.debug('Range orders placed');

  cf.info('Swaps Setup completed');
}

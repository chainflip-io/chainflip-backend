#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command for setting up new assets

import { Asset, runWithTimeoutAndExit } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';
import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from '../shared/deposit_liquidity';
import { rangeOrder } from '../shared/range_order';

export const deposits = new Map<Asset, number>([
  ['ArbUsdt', 1000000],
  ['Wbtc', 100],
  ['SolUsdt', 100000],
]);

export const price = new Map<Asset, number>([
  ['Wbtc', 10000],
  ['ArbUsdt', 1],
  ['SolUsdt', 1],
]);

export async function setupNewAssets<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up swaps for new assets: WBTC, ArbUsdt, SolUsdt');

  await Promise.all([
    createLpPool(cf.logger, 'Wbtc', price.get('Wbtc')!),
    createLpPool(cf.logger, 'ArbUsdt', price.get('ArbUsdt')!),
    createLpPool(cf.logger, 'SolUsdt', price.get('SolUsdt')!),
  ]);

  cf.info('Pools for WBTC, ArbUsdt, SolUsdt set');

  const lp1Deposits = (lpcf: ChainflipIO<A>) =>
    lpcf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Wbtc', deposits.get('Wbtc')!),
        (subcf) => depositLiquidity(subcf, 'ArbUsdt', deposits.get('ArbUsdt')!),
        (subcf) => depositLiquidity(subcf, 'SolUsdt', deposits.get('SolUsdt')!),
      ]);

  const lpApiDeposits = (lpcf: ChainflipIO<A>) =>
    lpcf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Wbtc', 10),
        (subcf) => depositLiquidity(subcf, 'ArbUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'SolUsdt', 1000),
      ]);

  await cf.all([lpApiDeposits, lp1Deposits]);

  cf.info('Lp1 deposits for WBTC, ArbUsdt, SolUsdt set');

  await Promise.all([
    rangeOrder(cf.logger, 'Wbtc', deposits.get('Wbtc')! * 0.9999),
    rangeOrder(cf.logger, 'ArbUsdt', deposits.get('ArbUsdt')! * 0.9999),
    rangeOrder(cf.logger, 'SolUsdt', deposits.get('SolUsdt')! * 0.9999),
  ]);

  cf.info('Range orders for WBTC, ArbUsdt, SolUsdt set');

  cf.info('Swaps Setup completed for new assets: WBTC, ArbUsdt, SolUsdt');
}


const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(setupNewAssets(cf), 240);

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command sets up the new Cbbtc asset (ERC-20 on Ethereum). Unlike a new chain,
// no vault initialization or refund address registration is needed: Ethereum is already
// active and the LPs registered their Ethereum refund addresses in the pre-upgrade setup.

import { runWithTimeoutAndExit } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { deposits, price } from 'shared/setup_swaps';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';

async function setupNewAsset<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up swaps for new asset: Cbbtc');

  await createLpPool(cf.logger, 'Cbbtc', price.get('Cbbtc')!);

  // Set permissive oracle slippage (100%) for the new pool to prevent swap failures in tests.
  await submitGovernanceExtrinsic((api) =>
    api.tx.swapping.updatePalletConfig([
      {
        type: 'SetDefaultOraclePriceSlippageProtectionForAsset' as const,
        value: {
          baseAsset: 'Cbbtc' as const,
          quoteAsset: 'Usdc' as const,
          bps: 10000,
        },
      },
    ]),
  );

  const lp1Deposits = (parentCf: ChainflipIO<A>) =>
    parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') }).all([
      // Fund the Usdc quote side of the Cbbtc range order below. LP_1's free Usdc
      // is nearly exhausted by the pre-upgrade setup's range orders.
      (subcf) => depositLiquidity(subcf, 'Usdc', 2000000),
      (subcf) => depositLiquidity(subcf, 'Cbbtc', deposits.get('Cbbtc')!),
    ]);

  const lpApiDeposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([(subcf) => depositLiquidity(subcf, 'Cbbtc', 10)]);

  cf.info('Depositing Cbbtc liquidity');
  await cf.all([lpApiDeposits, lp1Deposits]);

  const lp1RangeOrders = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([(subcf) => rangeOrder(subcf, 'Cbbtc', deposits.get('Cbbtc')! * 0.9999)]);

  cf.info('Setting up Cbbtc range order');
  await cf.all([lp1RangeOrders]);

  cf.debug('Range order placed');

  cf.info('Swaps Setup completed');
}

const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(Promise.all([setupNewAsset(cf)]), 500);

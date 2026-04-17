#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command for setting up new assets

import { runWithTimeoutAndExit, Asset, getTronWebClient } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { deposits, price } from 'shared/setup_swaps';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity, registerLiquidityRefundAddressForChain } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { initializeTronChain, initializeTronContracts } from 'shared/initialize_new_chains';
import { tronVaultAwaitingGovernanceActivation } from 'generated/events/tronVault/awaitingGovernanceActivation';
import { validatorNewEpoch } from 'generated/events/validator/newEpoch';

async function setupNewChain<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up vaults for Tron');
  const tronClient = getTronWebClient();

  // Step 1
  await initializeTronChain(cf.logger);

  // Step 2
  cf.info('Forcing rotation');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });

  const keyEvents = await cf.stepUntilAllEventsOf({
    tron: {
      name: 'TronVault.AwaitingGovernanceActivation',
      schema: tronVaultAwaitingGovernanceActivation,
    },
  });

  // Step 3
  cf.info('Waiting for new keys');
  const tronKey = keyEvents.tron.data.newPublicKey;

  // Step 4
  cf.info('Setting up external chain (Tron) with new keys');
  cf.info('Inserting Tron key in the contracts');
  await initializeTronContracts(tronClient, tronKey);
  cf.debug('Tron key inserted');

  // Confirmation
  cf.info('Waiting for new epoch...');
  await cf.stepUntilEvent('Validator.NewEpoch', validatorNewEpoch);
  cf.info('New Epoch');
  cf.info('Vault Setup completed');

  // Setup swaps
  cf.info('Setting up swaps for new assets: Trx and TrxUsdt');

  await Promise.all([
    createLpPool(cf.logger, 'Trx', price.get('Trx')!),
    createLpPool(cf.logger, 'TrxUsdt', price.get('TrxUsdt')!),
  ]);

  // Set permissive default oracle slippage (100%) for all pools to prevent swap failures in tests.
  // We do this for all assets, not just new ones, because the migration sets default values that
  // we want to override.
  await submitGovernanceExtrinsic((api) =>
    api.tx.swapping.updatePalletConfig(
      [...price.keys()]
        .filter((a): a is Asset => a !== 'Usdc')
        .map((asset) => ({
          SetDefaultOraclePriceSlippageProtectionForAsset: {
            baseAsset: asset,
            quoteAsset: 'Usdc',
            bps: 10000,
          },
        })),
    ),
  );

  cf.info('Registering Tron refund address');
  await cf.all(
    ['//LP_1', '//LP_API'].map(
      (uri) => (parentCf) =>
        parentCf
          .with({ account: fullAccountFromUri(uri as `//${string}`, 'LP') })
          .all([(subcf) => registerLiquidityRefundAddressForChain(subcf, 'Tron')]),
    ),
  );

  const lp1Deposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Trx', deposits.get('Trx')!),
        (subcf) => depositLiquidity(subcf, 'TrxUsdt', deposits.get('TrxUsdt')!),
      ]);

  const lpApiDeposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Trx', 10000),
        (subcf) => depositLiquidity(subcf, 'TrxUsdt', 1000),
      ]);

  cf.info('Depositing Tron liquidity');
  await cf.all([lpApiDeposits, lp1Deposits]);

  const lp1RangeOrders = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => rangeOrder(subcf, 'Trx', deposits.get('Trx')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'TrxUsdt', deposits.get('TrxUsdt')! * 0.9999),
      ]);

  cf.info('Setting up Trx and TrxUsdt range orders');
  await cf.all([lp1RangeOrders]);

  cf.debug('Range orders placed');

  cf.info('Swaps Setup completed');
}
const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(Promise.all([setupNewChain(cf)]), 360);

#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command for setting up new assets

import { runWithTimeoutAndExit, Asset, getWeb3, getContractAddress } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { deposits, price } from 'shared/setup_swaps';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity, registerLiquidityRefundAddressForChain } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { initializeBscChain, initializeBscContracts } from 'shared/initialize_new_chains';
import { getKeyManagerAbi } from 'shared/contract_interfaces';
import { bscVaultVaultRotatedExternallyEvent } from 'generated/events/bscVault/vaultRotatedExternally';

async function setupNewChain<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up vaults for Bsc');
  const bscClient = getWeb3('Bsc');

  // Step 1
  await initializeBscChain(cf.logger);

  // Step 2: BSC shares EvmCrypto with Ethereum/Arbitrum, so instead of forcing a validator
  // rotation to generate a brand-new aggregate key, we reuse the live EVM aggregate key. We
  // read it from the Ethereum KeyManager (which always holds the current active key, even
  // after rotations) and set it on the BSC KeyManager via the gov key.
  cf.info('Setting Bsc vault key to the current EVM aggregate key');
  const ethClient = getWeb3('Ethereum');
  const ethKeyManager = new ethClient.eth.Contract(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await getKeyManagerAbi()) as any,
    getContractAddress('Ethereum', 'KEY_MANAGER'),
  );
  const aggKey = await ethKeyManager.methods.getAggregateKey().call();

  cf.info('Inserting BSC key in the contracts');
  await initializeBscContracts(cf.logger, bscClient, {
    pubKeyX: aggKey.pubKeyX,
    pubKeyYParity: Number(aggKey.pubKeyYParity) === 1 ? 'Odd' : 'Even',
  });
  cf.debug('Bsc key inserted');

  // Step 3: the engine witnesses the gov-key set on the BSC KeyManager, which activates the
  // Bsc vault on the state chain via `inner_vault_key_rotated_externally`. No rotation needed.
  cf.info('Waiting for Bsc vault activation to be witnessed');
  await cf.stepUntilEvent(bscVaultVaultRotatedExternallyEvent);
  cf.info('Vault Setup completed');

  // Setup swaps
  cf.info('Setting up swaps for new assets: Bnb and BscUsdt');

  await Promise.all([
    createLpPool(cf.logger, 'Bnb', price.get('Bnb')!),
    createLpPool(cf.logger, 'BscUsdt', price.get('BscUsdt')!),
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

  cf.info('Registering Bsc refund address');
  await cf.all(
    ['//LP_1', '//LP_API'].map(
      (uri) => (parentCf) =>
        parentCf
          .with({ account: fullAccountFromUri(uri as `//${string}`, 'LP') })
          .all([(subcf) => registerLiquidityRefundAddressForChain(subcf, 'Bsc')]),
    ),
  );

  const lp1Deposits = (parentCf: ChainflipIO<A>) =>
    parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') }).all([
      // Fund the Usdc quote side of the Bnb + BscUsdt range orders below. LP_1's free Usdc
      // is nearly exhausted by the pre-upgrade setup's range orders.
      (subcf) => depositLiquidity(subcf, 'Usdc', 3000000),
      (subcf) => depositLiquidity(subcf, 'Bnb', deposits.get('Bnb')!),
      (subcf) => depositLiquidity(subcf, 'BscUsdt', deposits.get('BscUsdt')!),
    ]);

  const lpApiDeposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Bnb', 10),
        (subcf) => depositLiquidity(subcf, 'BscUsdt', 1000),
      ]);

  cf.info('Depositing Bsc liquidity');
  await cf.all([lpApiDeposits, lp1Deposits]);

  const lp1RangeOrders = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => rangeOrder(subcf, 'Bnb', deposits.get('Bnb')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'BscUsdt', deposits.get('BscUsdt')! * 0.9999),
      ]);

  cf.info('Setting up Bnb and BscUsdt range orders');
  await cf.all([lp1RangeOrders]);

  cf.debug('Range orders placed');

  cf.info('Swaps Setup completed');
}
const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(Promise.all([setupNewChain(cf)]), 500);

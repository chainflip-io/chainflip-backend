#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command for setting up new assets

import { getContractAddress, runWithTimeoutAndExit, decodeSolAddress, Asset } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';
import { PublicKey } from '@solana/web3.js';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { deposits, price } from 'shared/setup_swaps';
import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from '../shared/deposit_liquidity';
import { rangeOrder } from '../shared/range_order';

async function setupNewAssets<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up swaps for new assets: WBTC, ArbUsdt, SolUsdt');

  await Promise.all([
    createLpPool(cf.logger, 'Wbtc', price.get('Wbtc')!),
    createLpPool(cf.logger, 'ArbUsdt', price.get('ArbUsdt')!),
    createLpPool(cf.logger, 'SolUsdt', price.get('SolUsdt')!),
  ]);

  cf.info('Pools for WBTC, ArbUsdt, SolUsdt set');

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

async function addSolUsdtTokenSupport<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Adding token support for SolUsdt via governance');
  const tokenMintPubkey = new PublicKey(getContractAddress('Solana', 'SolUsdt'));

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.dispatchSolanaGovCall({
      SetTokenSwapParameters: {
        minSwapAmount: 5000000,
        tokenMintPubkey: decodeSolAddress(tokenMintPubkey.toString()),
      },
    }),
  );
  cf.info('Token support for SolUsdt added via governance');
}

const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(Promise.all([addSolUsdtTokenSupport(cf), setupNewAssets(cf)]), 240);

#!/usr/bin/env -S pnpm tsx
/**
 * Command: on_chain_swap
 *
 * Description:
 * Performs an on-chain swap (Swap using an LP's free balance) of a specified amount from one asset to another.
 * The default LP account used is LP_1. Set `LP_URI` environment variable to use a different LP account.
 *
 * Usage:
 * on_chain_swap <inputAsset> <outputAsset> <amount>
 *
 * Example:
 * ./commands/on_chain_swap.ts Eth Flip 20
 */
import { InternalAssets as Assets } from '@chainflip/cli';
import {
  amountToFineAmount,
  assetDecimals,
  createStateChainKeypair,
  handleSubstrateError,
  lpMutex,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { globalLogger as logger } from '../shared/utils/logger';
import { depositLiquidity } from '../shared/deposit_liquidity';

const args = process.argv.slice(2);
if (args.length < 3) {
  logger.error('Usage: on_chain_swap <inputAsset> <outputAsset> <amount>');
  process.exit(1);
}

const amount = parseFloat(args[2]);
const inputAsset = Assets[args[0] as keyof typeof Assets];
const outputAsset = Assets[args[1] as keyof typeof Assets];

if (!inputAsset || !outputAsset) {
  logger.error('Invalid asset provided. Valid assets are:', Object.keys(Assets).join(', '));
  process.exit(1);
}

await using chainflip = await getChainflipApi();
const lpUri = process.env.LP_URI || '//LP_1';
const lp = createStateChainKeypair(lpUri);

logger.info('Depositing liquidity on account');
await depositLiquidity(logger, inputAsset, amount, false, lpUri);

const swapEvent = observeEvent(logger, `swapping:CreditedOnChain`, {
  test: (event) => event.data.accountId === lp.address,
}).event;

logger.info('Submitting on-chain swap extrinsic');
await lpMutex.runExclusive(async () => {
  await chainflip.tx.liquidityProvider
    .onChainSwap(
      amountToFineAmount(amount.toString(), assetDecimals(inputAsset)),
      inputAsset,
      outputAsset,
      0,
      0,
      undefined,
    )
    .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
});

await swapEvent;
logger.info('✅ On-chain swap completed');

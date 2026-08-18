import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderFlipTransferredToOnChainBalance = z.object({
  accountId,
  amount: numberOrHex,
});

export const liquidityProviderFlipTransferredToOnChainBalanceEvent = defineEvent(
  'LiquidityProvider.FlipTransferredToOnChainBalance',
  liquidityProviderFlipTransferredToOnChainBalance,
);

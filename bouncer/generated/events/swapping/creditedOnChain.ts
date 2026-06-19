import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingCreditedOnChain = z.object({
  swapRequestId: numberOrHex,
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
});

export const swappingCreditedOnChainEvent = defineEvent(
  'Swapping.CreditedOnChain',
  swappingCreditedOnChain,
);

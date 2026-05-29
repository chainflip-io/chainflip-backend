import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapExecuted = z.object({
  swapRequestId: numberOrHex,
  swapId: numberOrHex,
  inputAsset: cfPrimitivesChainsAssetsAnyAsset,
  outputAsset: cfPrimitivesChainsAssetsAnyAsset,
  inputAmount: numberOrHex,
  networkFee: numberOrHex,
  brokerFee: numberOrHex,
  intermediateAmount: numberOrHex.nullish(),
  outputAmount: numberOrHex,
  oracleDelta: z.number().nullish(),
  oracleDeltaExFees: z.number().nullish(),
});

export const swappingSwapExecutedEvent = defineEvent('Swapping.SwapExecuted', swappingSwapExecuted);

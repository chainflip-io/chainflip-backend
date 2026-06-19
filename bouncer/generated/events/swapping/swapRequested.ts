import { z } from 'zod';
import {
  cfChainsSwapOrigin,
  cfPrimitivesBeneficiaryAccountId32,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesDcaParameters,
  cfTraitsSwappingPriceLimitsAndExpiry,
  cfTraitsSwappingSwapRequestTypeGeneric,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapRequested = z.object({
  swapRequestId: numberOrHex,
  inputAsset: cfPrimitivesChainsAssetsAnyAsset,
  inputAmount: numberOrHex,
  outputAsset: cfPrimitivesChainsAssetsAnyAsset,
  origin: cfChainsSwapOrigin,
  requestType: cfTraitsSwappingSwapRequestTypeGeneric,
  brokerFees: z.array(cfPrimitivesBeneficiaryAccountId32),
  priceLimitsAndExpiry: cfTraitsSwappingPriceLimitsAndExpiry.nullish(),
  dcaParameters: cfPrimitivesDcaParameters.nullish(),
});

export const swappingSwapRequestedEvent = defineEvent(
  'Swapping.SwapRequested',
  swappingSwapRequested,
);

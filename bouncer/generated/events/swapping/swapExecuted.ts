import { z } from 'zod';
import { cfPrimitivesAssetAndAmount, numberOrHex } from '../common';

export const swappingSwapExecuted = z.object({
  swapRequestId: numberOrHex,
  swapId: numberOrHex,
  input: cfPrimitivesAssetAndAmount,
  output: cfPrimitivesAssetAndAmount,
  networkFee: cfPrimitivesAssetAndAmount,
  brokerFee: cfPrimitivesAssetAndAmount,
  intermediate: cfPrimitivesAssetAndAmount.nullish(),
  oracleDelta: z.number().nullish(),
  oracleDeltaExFees: z.number().nullish(),
});

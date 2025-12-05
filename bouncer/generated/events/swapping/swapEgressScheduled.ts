import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';

export const swappingSwapEgressScheduled = z.object({
  swapRequestId: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  egressFee: z.tuple([numberOrHex, cfPrimitivesChainsAssetsAnyAsset]),
});

import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsArbAsset,
  cfTraitsScheduledEgressDetailsArbitrum,
  hexString,
  numberOrHex,
} from '../common';

export const arbitrumIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsArbAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsArbitrum.nullish(),
});

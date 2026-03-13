import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsBscAsset,
  cfTraitsScheduledEgressDetailsBsc,
  hexString,
  numberOrHex,
} from '../common';

export const bscIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsBsc.nullish(),
});

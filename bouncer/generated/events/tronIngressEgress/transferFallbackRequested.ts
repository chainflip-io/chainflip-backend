import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsTronAsset,
  cfTraitsScheduledEgressDetailsTron,
  hexString,
  numberOrHex,
} from '../common';

export const tronIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsTron.nullish(),
});

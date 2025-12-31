import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsHubAsset,
  cfTraitsScheduledEgressDetailsAssethub,
  hexString,
  numberOrHex,
} from '../common';

export const assethubIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsAssethub.nullish(),
});

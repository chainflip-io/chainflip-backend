import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsEthAsset,
  cfTraitsScheduledEgressDetailsEthereum,
  hexString,
  numberOrHex,
} from '../common';

export const ethereumIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsEthereum.nullish(),
});

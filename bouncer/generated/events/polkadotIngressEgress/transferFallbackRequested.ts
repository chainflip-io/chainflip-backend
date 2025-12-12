import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsDotAsset,
  cfTraitsScheduledEgressDetailsPolkadot,
  hexString,
  numberOrHex,
} from '../common';

export const polkadotIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsDotAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsPolkadot.nullish(),
});

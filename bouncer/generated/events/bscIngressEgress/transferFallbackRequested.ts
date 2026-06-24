import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsBscAsset,
  cfTraitsScheduledEgressDetailsBsc,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsBscAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsBsc.nullish(),
});

export const bscIngressEgressTransferFallbackRequestedEvent = defineEvent(
  'BscIngressEgress.TransferFallbackRequested',
  bscIngressEgressTransferFallbackRequested,
);

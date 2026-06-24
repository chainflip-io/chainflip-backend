import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsTronAsset,
  cfTraitsScheduledEgressDetailsTron,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsTronAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsTron.nullish(),
});

export const tronIngressEgressTransferFallbackRequestedEvent = defineEvent(
  'TronIngressEgress.TransferFallbackRequested',
  tronIngressEgressTransferFallbackRequested,
);

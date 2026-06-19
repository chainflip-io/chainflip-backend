import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsHubAsset,
  cfTraitsScheduledEgressDetailsAssethub,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsHubAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsAssethub.nullish(),
});

export const assethubIngressEgressTransferFallbackRequestedEvent = defineEvent(
  'AssethubIngressEgress.TransferFallbackRequested',
  assethubIngressEgressTransferFallbackRequested,
);

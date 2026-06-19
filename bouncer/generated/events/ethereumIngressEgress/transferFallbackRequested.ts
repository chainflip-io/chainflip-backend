import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsEthAsset,
  cfTraitsScheduledEgressDetailsEthereum,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransferFallbackRequested = z.object({
  asset: cfPrimitivesChainsAssetsEthAsset,
  amount: numberOrHex,
  destinationAddress: hexString,
  broadcastId: z.number(),
  egressDetails: cfTraitsScheduledEgressDetailsEthereum.nullish(),
});

export const ethereumIngressEgressTransferFallbackRequestedEvent = defineEvent(
  'EthereumIngressEgress.TransferFallbackRequested',
  ethereumIngressEgressTransferFallbackRequested,
);

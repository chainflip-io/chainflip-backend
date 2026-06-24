import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});

export const bitcoinIngressEgressBatchBroadcastRequestedEvent = defineEvent(
  'BitcoinIngressEgress.BatchBroadcastRequested',
  bitcoinIngressEgressBatchBroadcastRequested,
);

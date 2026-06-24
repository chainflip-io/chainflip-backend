import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});

export const ethereumIngressEgressBatchBroadcastRequestedEvent = defineEvent(
  'EthereumIngressEgress.BatchBroadcastRequested',
  ethereumIngressEgressBatchBroadcastRequested,
);

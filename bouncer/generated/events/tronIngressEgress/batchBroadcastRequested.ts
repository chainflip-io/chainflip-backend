import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});

export const tronIngressEgressBatchBroadcastRequestedEvent = defineEvent(
  'TronIngressEgress.BatchBroadcastRequested',
  tronIngressEgressBatchBroadcastRequested,
);

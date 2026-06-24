import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});

export const solanaIngressEgressBatchBroadcastRequestedEvent = defineEvent(
  'SolanaIngressEgress.BatchBroadcastRequested',
  solanaIngressEgressBatchBroadcastRequested,
);

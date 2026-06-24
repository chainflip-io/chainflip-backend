import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});

export const polkadotIngressEgressBatchBroadcastRequestedEvent = defineEvent(
  'PolkadotIngressEgress.BatchBroadcastRequested',
  polkadotIngressEgressBatchBroadcastRequested,
);

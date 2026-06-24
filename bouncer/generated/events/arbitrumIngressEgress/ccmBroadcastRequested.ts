import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressCcmBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});

export const arbitrumIngressEgressCcmBroadcastRequestedEvent = defineEvent(
  'ArbitrumIngressEgress.CcmBroadcastRequested',
  arbitrumIngressEgressCcmBroadcastRequested,
);

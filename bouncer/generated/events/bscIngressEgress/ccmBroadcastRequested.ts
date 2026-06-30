import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressCcmBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});

export const bscIngressEgressCcmBroadcastRequestedEvent = defineEvent(
  'BscIngressEgress.CcmBroadcastRequested',
  bscIngressEgressCcmBroadcastRequested,
);

import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressCcmBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});

export const tronIngressEgressCcmBroadcastRequestedEvent = defineEvent(
  'TronIngressEgress.CcmBroadcastRequested',
  tronIngressEgressCcmBroadcastRequested,
);

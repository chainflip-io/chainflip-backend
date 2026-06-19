import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const tronIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'TronIngressEgress.FailedForeignChainCallResigned',
  tronIngressEgressFailedForeignChainCallResigned,
);

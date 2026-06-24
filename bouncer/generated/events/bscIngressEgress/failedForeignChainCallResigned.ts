import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const bscIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'BscIngressEgress.FailedForeignChainCallResigned',
  bscIngressEgressFailedForeignChainCallResigned,
);

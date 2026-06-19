import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const arbitrumIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'ArbitrumIngressEgress.FailedForeignChainCallResigned',
  arbitrumIngressEgressFailedForeignChainCallResigned,
);

import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const assethubIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'AssethubIngressEgress.FailedForeignChainCallResigned',
  assethubIngressEgressFailedForeignChainCallResigned,
);

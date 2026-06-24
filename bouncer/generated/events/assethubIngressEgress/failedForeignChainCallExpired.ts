import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const assethubIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'AssethubIngressEgress.FailedForeignChainCallExpired',
  assethubIngressEgressFailedForeignChainCallExpired,
);

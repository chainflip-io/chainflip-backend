import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const arbitrumIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'ArbitrumIngressEgress.FailedForeignChainCallExpired',
  arbitrumIngressEgressFailedForeignChainCallExpired,
);

import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const ethereumIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'EthereumIngressEgress.FailedForeignChainCallExpired',
  ethereumIngressEgressFailedForeignChainCallExpired,
);

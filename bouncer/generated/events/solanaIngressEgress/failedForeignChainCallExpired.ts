import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const solanaIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'SolanaIngressEgress.FailedForeignChainCallExpired',
  solanaIngressEgressFailedForeignChainCallExpired,
);

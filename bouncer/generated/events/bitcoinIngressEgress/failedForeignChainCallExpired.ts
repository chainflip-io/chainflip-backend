import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const bitcoinIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'BitcoinIngressEgress.FailedForeignChainCallExpired',
  bitcoinIngressEgressFailedForeignChainCallExpired,
);

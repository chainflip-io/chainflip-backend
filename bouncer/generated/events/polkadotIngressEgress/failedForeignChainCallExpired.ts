import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

export const polkadotIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'PolkadotIngressEgress.FailedForeignChainCallExpired',
  polkadotIngressEgressFailedForeignChainCallExpired,
);

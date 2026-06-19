import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedForeignChainCallExpired = z.object({ broadcastId: z.number() });

export const tronIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'TronIngressEgress.FailedForeignChainCallExpired',
  tronIngressEgressFailedForeignChainCallExpired,
);

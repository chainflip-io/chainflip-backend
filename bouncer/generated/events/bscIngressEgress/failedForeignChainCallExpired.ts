import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedForeignChainCallExpired = z.object({ broadcastId: z.number() });

export const bscIngressEgressFailedForeignChainCallExpiredEvent = defineEvent(
  'BscIngressEgress.FailedForeignChainCallExpired',
  bscIngressEgressFailedForeignChainCallExpired,
);

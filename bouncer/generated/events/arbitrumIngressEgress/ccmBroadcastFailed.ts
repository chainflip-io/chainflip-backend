import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const arbitrumIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'ArbitrumIngressEgress.CcmBroadcastFailed',
  arbitrumIngressEgressCcmBroadcastFailed,
);

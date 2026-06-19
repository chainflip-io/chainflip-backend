import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const assethubIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'AssethubIngressEgress.CcmBroadcastFailed',
  assethubIngressEgressCcmBroadcastFailed,
);

import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const bscIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'BscIngressEgress.CcmBroadcastFailed',
  bscIngressEgressCcmBroadcastFailed,
);

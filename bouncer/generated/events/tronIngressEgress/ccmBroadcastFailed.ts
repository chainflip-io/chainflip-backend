import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const tronIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'TronIngressEgress.CcmBroadcastFailed',
  tronIngressEgressCcmBroadcastFailed,
);

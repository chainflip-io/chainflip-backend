import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const solanaIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'SolanaIngressEgress.CcmBroadcastFailed',
  solanaIngressEgressCcmBroadcastFailed,
);

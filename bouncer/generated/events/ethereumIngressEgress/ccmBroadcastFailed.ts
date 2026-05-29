import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const ethereumIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'EthereumIngressEgress.CcmBroadcastFailed',
  ethereumIngressEgressCcmBroadcastFailed,
);

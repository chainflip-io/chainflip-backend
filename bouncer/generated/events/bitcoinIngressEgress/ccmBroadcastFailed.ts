import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const bitcoinIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'BitcoinIngressEgress.CcmBroadcastFailed',
  bitcoinIngressEgressCcmBroadcastFailed,
);

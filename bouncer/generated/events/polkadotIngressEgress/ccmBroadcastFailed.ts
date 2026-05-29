import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

export const polkadotIngressEgressCcmBroadcastFailedEvent = defineEvent(
  'PolkadotIngressEgress.CcmBroadcastFailed',
  polkadotIngressEgressCcmBroadcastFailed,
);

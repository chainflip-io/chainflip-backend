import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const bitcoinIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'BitcoinIngressEgress.FailedForeignChainCallResigned',
  bitcoinIngressEgressFailedForeignChainCallResigned,
);

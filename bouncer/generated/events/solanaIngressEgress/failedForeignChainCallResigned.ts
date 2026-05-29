import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const solanaIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'SolanaIngressEgress.FailedForeignChainCallResigned',
  solanaIngressEgressFailedForeignChainCallResigned,
);

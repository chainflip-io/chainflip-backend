import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const ethereumIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'EthereumIngressEgress.FailedForeignChainCallResigned',
  ethereumIngressEgressFailedForeignChainCallResigned,
);

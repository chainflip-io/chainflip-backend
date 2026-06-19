import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

export const polkadotIngressEgressFailedForeignChainCallResignedEvent = defineEvent(
  'PolkadotIngressEgress.FailedForeignChainCallResigned',
  polkadotIngressEgressFailedForeignChainCallResigned,
);

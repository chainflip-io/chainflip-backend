import { z } from 'zod';

export const arbitrumIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

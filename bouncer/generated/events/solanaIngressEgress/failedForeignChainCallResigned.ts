import { z } from 'zod';

export const solanaIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

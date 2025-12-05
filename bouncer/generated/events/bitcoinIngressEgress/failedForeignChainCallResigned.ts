import { z } from 'zod';

export const bitcoinIngressEgressFailedForeignChainCallResigned = z.object({
  broadcastId: z.number(),
  thresholdSignatureId: z.number(),
});

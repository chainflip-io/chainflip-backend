import { z } from 'zod';

export const bitcoinIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

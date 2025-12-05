import { z } from 'zod';

export const arbitrumIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

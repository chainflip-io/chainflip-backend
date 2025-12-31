import { z } from 'zod';

export const ethereumIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

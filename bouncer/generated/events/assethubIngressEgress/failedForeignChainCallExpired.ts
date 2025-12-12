import { z } from 'zod';

export const assethubIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

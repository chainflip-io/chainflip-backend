import { z } from 'zod';

export const solanaIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});

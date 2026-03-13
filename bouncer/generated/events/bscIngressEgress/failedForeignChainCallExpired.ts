import { z } from 'zod';

export const bscIngressEgressFailedForeignChainCallExpired = z.object({ broadcastId: z.number() });

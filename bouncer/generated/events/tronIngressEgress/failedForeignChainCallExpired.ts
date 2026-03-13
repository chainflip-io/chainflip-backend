import { z } from 'zod';

export const tronIngressEgressFailedForeignChainCallExpired = z.object({ broadcastId: z.number() });

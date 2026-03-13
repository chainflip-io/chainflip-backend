import { z } from 'zod';

export const tronIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

import { z } from 'zod';

export const ethereumIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

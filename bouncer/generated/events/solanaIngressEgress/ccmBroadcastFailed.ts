import { z } from 'zod';

export const solanaIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

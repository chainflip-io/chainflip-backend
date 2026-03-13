import { z } from 'zod';

export const bscIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

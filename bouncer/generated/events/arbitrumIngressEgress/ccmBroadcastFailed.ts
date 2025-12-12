import { z } from 'zod';

export const arbitrumIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

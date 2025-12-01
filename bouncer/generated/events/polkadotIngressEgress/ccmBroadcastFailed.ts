import { z } from 'zod';

export const polkadotIngressEgressCcmBroadcastFailed = z.object({ broadcastId: z.number() });

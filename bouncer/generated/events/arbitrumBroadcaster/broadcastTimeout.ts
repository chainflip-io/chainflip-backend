import { z } from 'zod';

export const arbitrumBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

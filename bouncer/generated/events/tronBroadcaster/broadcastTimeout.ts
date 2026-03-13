import { z } from 'zod';

export const tronBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

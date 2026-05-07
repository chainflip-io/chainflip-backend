import { z } from 'zod';

export const tronBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

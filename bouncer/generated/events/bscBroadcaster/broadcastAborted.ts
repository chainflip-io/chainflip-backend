import { z } from 'zod';

export const bscBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

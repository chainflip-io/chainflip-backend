import { z } from 'zod';

export const assethubBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

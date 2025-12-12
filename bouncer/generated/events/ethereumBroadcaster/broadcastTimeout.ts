import { z } from 'zod';

export const ethereumBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

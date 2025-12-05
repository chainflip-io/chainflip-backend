import { z } from 'zod';

export const assethubBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

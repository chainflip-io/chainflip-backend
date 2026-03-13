import { z } from 'zod';

export const bscBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

import { z } from 'zod';

export const bscBroadcasterCallResigned = z.object({ broadcastId: z.number() });

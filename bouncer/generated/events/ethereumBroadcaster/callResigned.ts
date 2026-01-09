import { z } from 'zod';

export const ethereumBroadcasterCallResigned = z.object({ broadcastId: z.number() });

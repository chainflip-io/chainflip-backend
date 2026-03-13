import { z } from 'zod';

export const tronBroadcasterCallResigned = z.object({ broadcastId: z.number() });

import { z } from 'zod';

export const arbitrumBroadcasterCallResigned = z.object({ broadcastId: z.number() });

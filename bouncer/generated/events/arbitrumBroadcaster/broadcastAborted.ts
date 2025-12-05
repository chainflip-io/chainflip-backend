import { z } from 'zod';

export const arbitrumBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

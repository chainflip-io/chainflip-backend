import { z } from 'zod';

export const bitcoinBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

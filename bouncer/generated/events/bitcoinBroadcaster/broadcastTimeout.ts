import { z } from 'zod';

export const bitcoinBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

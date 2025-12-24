import { z } from 'zod';

export const bitcoinBroadcasterCallResigned = z.object({ broadcastId: z.number() });

import { z } from 'zod';

export const polkadotBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

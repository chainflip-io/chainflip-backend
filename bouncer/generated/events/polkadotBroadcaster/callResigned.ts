import { z } from 'zod';

export const polkadotBroadcasterCallResigned = z.object({ broadcastId: z.number() });

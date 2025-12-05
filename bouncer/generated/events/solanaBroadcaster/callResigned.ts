import { z } from 'zod';

export const solanaBroadcasterCallResigned = z.object({ broadcastId: z.number() });

import { z } from 'zod';

export const solanaBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

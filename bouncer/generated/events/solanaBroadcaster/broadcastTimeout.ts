import { z } from 'zod';

export const solanaBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

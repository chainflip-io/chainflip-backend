import { z } from 'zod';
import { dispatchResult } from '../common';

export const bitcoinBroadcasterBroadcastCallbackExecuted = z.object({
  broadcastId: z.number(),
  result: dispatchResult,
});

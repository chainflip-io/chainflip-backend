import { z } from 'zod';
import { dispatchResult } from '../common';

export const arbitrumBroadcasterBroadcastCallbackExecuted = z.object({
  broadcastId: z.number(),
  result: dispatchResult,
});

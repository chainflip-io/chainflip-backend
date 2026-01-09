import { z } from 'zod';
import { dispatchResult } from '../common';

export const ethereumBroadcasterBroadcastCallbackExecuted = z.object({
  broadcastId: z.number(),
  result: dispatchResult,
});

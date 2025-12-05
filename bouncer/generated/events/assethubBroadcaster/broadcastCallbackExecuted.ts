import { z } from 'zod';
import { dispatchResult } from '../common';

export const assethubBroadcasterBroadcastCallbackExecuted = z.object({
  broadcastId: z.number(),
  result: dispatchResult,
});

import { z } from 'zod';
import { dispatchResult } from '../common';

export const solanaBroadcasterBroadcastCallbackExecuted = z.object({
  broadcastId: z.number(),
  result: dispatchResult,
});

import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const liquidityPoolsLimitOrderSetOrUpdateScheduled = z.object({
  lp: accountId,
  orderId: numberOrHex,
  dispatchAt: z.number(),
});

import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const liquidityPoolsScheduledLimitOrderUpdateDispatchSuccess = z.object({
  lp: accountId,
  orderId: numberOrHex,
});

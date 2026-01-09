import { z } from 'zod';
import { accountId, numberOrHex, spRuntimeDispatchError } from '../common';

export const liquidityPoolsScheduledLimitOrderUpdateDispatchFailure = z.object({
  lp: accountId,
  orderId: numberOrHex,
  error: spRuntimeDispatchError,
});

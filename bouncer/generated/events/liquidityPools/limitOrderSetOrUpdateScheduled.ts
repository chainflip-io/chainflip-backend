import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsLimitOrderSetOrUpdateScheduled = z.object({
  lp: accountId,
  orderId: numberOrHex,
  dispatchAt: z.number(),
});

export const liquidityPoolsLimitOrderSetOrUpdateScheduledEvent = defineEvent(
  'LiquidityPools.LimitOrderSetOrUpdateScheduled',
  liquidityPoolsLimitOrderSetOrUpdateScheduled,
);

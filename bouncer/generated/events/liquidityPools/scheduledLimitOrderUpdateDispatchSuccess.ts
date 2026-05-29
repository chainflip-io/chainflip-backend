import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsScheduledLimitOrderUpdateDispatchSuccess = z.object({
  lp: accountId,
  orderId: numberOrHex,
});

export const liquidityPoolsScheduledLimitOrderUpdateDispatchSuccessEvent = defineEvent(
  'LiquidityPools.ScheduledLimitOrderUpdateDispatchSuccess',
  liquidityPoolsScheduledLimitOrderUpdateDispatchSuccess,
);

import { z } from 'zod';
import { accountId, numberOrHex, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsScheduledLimitOrderUpdateDispatchFailure = z.object({
  lp: accountId,
  orderId: numberOrHex,
  error: spRuntimeDispatchError,
});

export const liquidityPoolsScheduledLimitOrderUpdateDispatchFailureEvent = defineEvent(
  'LiquidityPools.ScheduledLimitOrderUpdateDispatchFailure',
  liquidityPoolsScheduledLimitOrderUpdateDispatchFailure,
);

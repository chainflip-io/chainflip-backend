import { z } from 'zod';
import {
  accountId,
  palletCfLendingPoolsGeneralLendingLiquidationCompletionReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLiquidationCompleted = z.object({
  borrowerId: accountId,
  reason: palletCfLendingPoolsGeneralLendingLiquidationCompletionReason,
});

export const lendingPoolsLiquidationCompletedEvent = defineEvent(
  'LendingPools.LiquidationCompleted',
  lendingPoolsLiquidationCompleted,
);

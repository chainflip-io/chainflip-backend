import { z } from 'zod';
import {
  accountId,
  palletCfLendingPoolsGeneralLendingLiquidationCompletionReason,
} from '../common';

export const lendingPoolsLiquidationCompleted = z.object({
  borrowerId: accountId,
  reason: palletCfLendingPoolsGeneralLendingLiquidationCompletionReason,
});

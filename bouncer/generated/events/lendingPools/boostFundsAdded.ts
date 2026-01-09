import { z } from 'zod';
import { accountId, numberOrHex, palletCfLendingPoolsBoostBoostPoolId } from '../common';

export const lendingPoolsBoostFundsAdded = z.object({
  boosterId: accountId,
  boostPool: palletCfLendingPoolsBoostBoostPoolId,
  amount: numberOrHex,
});

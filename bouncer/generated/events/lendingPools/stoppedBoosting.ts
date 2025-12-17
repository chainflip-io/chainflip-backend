import { z } from 'zod';
import { accountId, numberOrHex, palletCfLendingPoolsBoostBoostPoolId } from '../common';

export const lendingPoolsStoppedBoosting = z.object({
  boosterId: accountId,
  boostPool: palletCfLendingPoolsBoostBoostPoolId,
  unlockedAmount: numberOrHex,
  pendingBoosts: z.array(numberOrHex),
});

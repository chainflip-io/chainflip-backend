import { z } from 'zod';
import { palletCfLendingPoolsBoostBoostPoolId } from '../common';

export const lendingPoolsBoostPoolCreated = z.object({
  boostPool: palletCfLendingPoolsBoostBoostPoolId,
});

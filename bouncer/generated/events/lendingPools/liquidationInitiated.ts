import { z } from 'zod';
import {
  accountId,
  numberOrHex,
  palletCfLendingPoolsGeneralLendingLiquidationType,
} from '../common';

export const lendingPoolsLiquidationInitiated = z.object({
  borrowerId: accountId,
  swaps: z.array(z.tuple([numberOrHex, z.array(numberOrHex)])),
  liquidationType: palletCfLendingPoolsGeneralLendingLiquidationType,
});

import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsCollateralAddedActionType,
} from '../common';

export const lendingPoolsCollateralAdded = z.object({
  borrowerId: accountId,
  collateral: z.array(z.tuple([cfPrimitivesChainsAssetsAnyAsset, numberOrHex])),
  actionType: palletCfLendingPoolsCollateralAddedActionType,
});

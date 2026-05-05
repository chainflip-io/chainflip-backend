import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsSupplyRemovedActionType,
} from '../common';

export const lendingPoolsLendingFundsRemoved = z.object({
  lenderId: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  unlockedAmount: numberOrHex,
  actionType: palletCfLendingPoolsSupplyRemovedActionType,
});

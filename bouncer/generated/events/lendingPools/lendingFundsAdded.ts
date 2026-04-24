import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsSupplyAddedActionType,
} from '../common';

export const lendingPoolsLendingFundsAdded = z.object({
  lenderId: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  actionType: palletCfLendingPoolsSupplyAddedActionType,
});
